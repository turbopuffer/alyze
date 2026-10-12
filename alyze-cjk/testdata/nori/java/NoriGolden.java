// Golden-file generator for the Rust port of Lucene's nori (Korean) analyzer.
//
// Runs the reference Java implementation (lucene-analysis-nori, as used by Elasticsearch's
// analysis-nori plugin) over a set of inputs and writes out exactly what it produced, so the Rust
// port can be checked against it without a JVM at test time. See ../README.md for the file
// formats and ../gen.sh for how this is compiled and run.
//
// Modes (options are key=value pairs, see parseOptions):
//   tokens    <cases> <out> [opts]   KoreanTokenizer output (ES `nori_tokenizer`)
//   analyze   <cases> <out> [opts]   KoreanAnalyzer output (ES `nori` analyzer)
//   chain     <cases> <out> filters=<spec> [opts]   tokenizer + a custom filter chain
//   chardef   <out>                  CharacterDefinition class of every UTF-16 code unit + flags
//   unicode   <out>                  JDK general category / script / isDigit per code unit
//   lowercase <out>                  Character.toLowerCase(int) for every code point it changes
//   dict      <probes> <out> [dump=<dir>]   dictionary checksums + probe lookups (+ full dump)
//   userdict  <rules> <out>          what UserDictionary.open builds from a rules file

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.DataOutputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.PrintWriter;
import java.io.Reader;
import java.io.StringReader;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.EnumSet;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.analysis.LowerCaseFilter;
import org.apache.lucene.analysis.TokenStream;
import org.apache.lucene.analysis.ko.KoreanAnalyzer;
import org.apache.lucene.analysis.ko.KoreanNumberFilter;
import org.apache.lucene.analysis.ko.KoreanPartOfSpeechStopFilter;
import org.apache.lucene.analysis.ko.KoreanReadingFormFilter;
import org.apache.lucene.analysis.ko.KoreanTokenizer;
import org.apache.lucene.analysis.ko.POS;
import org.apache.lucene.analysis.ko.Token;
import org.apache.lucene.analysis.ko.dict.CharacterDefinition;
import org.apache.lucene.analysis.ko.dict.ConnectionCosts;
import org.apache.lucene.analysis.ko.dict.KoMorphData;
import org.apache.lucene.analysis.ko.dict.TokenInfoDictionary;
import org.apache.lucene.analysis.ko.dict.UnknownDictionary;
import org.apache.lucene.analysis.ko.dict.UserDictionary;
import org.apache.lucene.analysis.ko.tokenattributes.PartOfSpeechAttribute;
import org.apache.lucene.analysis.ko.tokenattributes.ReadingAttribute;
import org.apache.lucene.analysis.morph.TokenType;
import org.apache.lucene.analysis.tokenattributes.CharTermAttribute;
import org.apache.lucene.analysis.tokenattributes.OffsetAttribute;
import org.apache.lucene.analysis.tokenattributes.PositionIncrementAttribute;
import org.apache.lucene.analysis.tokenattributes.PositionLengthAttribute;
import org.apache.lucene.util.IntsRef;
import org.apache.lucene.util.fst.FST;
import org.apache.lucene.util.fst.IntsRefFSTEnum;

@SuppressWarnings("unchecked")
public final class NoriGolden {
  public static void main(String[] args) throws Exception {
    String mode = args[0];
    switch (mode) {
      case "tokens", "analyze", "chain" -> {
        List<String> inputs = readCases(args[1]);
        Map<String, String> opts = parseOptions(args, 3);
        try (PrintWriter out = writer(args[2])) {
          switch (mode) {
            case "tokens" -> tokens(inputs, opts, out);
            case "analyze" -> analyze(inputs, opts, out);
            default -> chain(inputs, opts, out);
          }
        }
      }
      case "chardef" -> {
        try (PrintWriter out = writer(args[1])) {
          charDef(out);
        }
      }
      case "unicode" -> {
        try (PrintWriter out = writer(args[1])) {
          unicode(out);
        }
      }
      case "lowercase" -> {
        try (PrintWriter out = writer(args[1])) {
          lowercase(out);
        }
      }
      case "dict" -> {
        Map<String, String> opts = parseOptions(args, 3);
        try (PrintWriter out = writer(args[2])) {
          dict(readCases(args[1]), opts.get("dump"), out);
        }
      }
      case "userdict" -> {
        try (PrintWriter out = writer(args[2])) {
          userDict(args[1], out);
        }
      }
      default -> throw new IllegalArgumentException("unknown mode: " + mode);
    }
  }

  private static Map<String, String> parseOptions(String[] args, int from) {
    Map<String, String> opts = new HashMap<>();
    for (int i = from; i < args.length; i++) {
      int eq = args[i].indexOf('=');
      if (eq < 0) throw new IllegalArgumentException("expected key=value: " + args[i]);
      opts.put(args[i].substring(0, eq), args[i].substring(eq + 1));
    }
    return opts;
  }

  // ---------------------------------------------------------------------------------------------
  // Tokenizer / analyzer / chain construction. Options:
  //   decompound=none|discard|mixed   (default discard, like ES)
  //   punctuation=keep|discard        (default discard, like ES)
  //   unigrams=true|false             (default false; not reachable from ES)
  //   userdict=<rules file>           (default none)
  //   stoptags=TAG,TAG|none           (analyze only; default: KoreanPartOfSpeechStopFilter defaults)
  //   filters=<spec>                  (chain only; comma-separated: pos[:TAG+TAG], reading,
  //                                    number, lowercase; applied in order)

  private static KoreanTokenizer.DecompoundMode decompoundMode(Map<String, String> opts) {
    return KoreanTokenizer.DecompoundMode.valueOf(
        opts.getOrDefault("decompound", "discard").toUpperCase(Locale.ROOT));
  }

  private static UserDictionary userDictionary(Map<String, String> opts) throws IOException {
    String path = opts.get("userdict");
    if (path == null) return null;
    try (Reader reader = Files.newBufferedReader(Paths.get(path), StandardCharsets.UTF_8)) {
      return UserDictionary.open(reader);
    }
  }

  private static Set<POS.Tag> stopTags(String spec) {
    if (spec == null) return KoreanPartOfSpeechStopFilter.DEFAULT_STOP_TAGS;
    if (spec.equals("none")) return Collections.emptySet();
    Set<POS.Tag> tags = EnumSet.noneOf(POS.Tag.class);
    for (String tag : spec.split("[,+]")) tags.add(POS.resolveTag(tag.trim()));
    return tags;
  }

  private static KoreanTokenizer newTokenizer(Map<String, String> opts) throws IOException {
    return new KoreanTokenizer(
        TokenStream.DEFAULT_TOKEN_ATTRIBUTE_FACTORY,
        userDictionary(opts),
        decompoundMode(opts),
        Boolean.parseBoolean(opts.getOrDefault("unigrams", "false")),
        !opts.getOrDefault("punctuation", "discard").equals("keep"));
  }

  private static void tokens(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    KoreanTokenizer tokenizer = newTokenizer(opts);
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      tokenizer.setReader(new StringReader(input));
      out.println("# " + i);
      dumpStream(tokenizer, input, out);
    }
  }

  private static void analyze(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    // Same construction as Elasticsearch's NoriAnalyzerProvider.
    try (Analyzer analyzer =
        new KoreanAnalyzer(
            userDictionary(opts), decompoundMode(opts), stopTags(opts.get("stoptags")), false)) {
      for (int i = 0; i < inputs.size(); i++) {
        String input = inputs.get(i);
        out.println("# " + i);
        dumpStream(analyzer.tokenStream("", input), input, out);
      }
    }
  }

  private static void chain(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    String spec = opts.get("filters");
    if (spec == null) throw new IllegalArgumentException("chain needs filters=...");
    KoreanTokenizer tokenizer = newTokenizer(opts);
    TokenStream stream = tokenizer;
    for (String filter : spec.split(",")) {
      String name = filter;
      String arg = null;
      int colon = filter.indexOf(':');
      if (colon >= 0) {
        name = filter.substring(0, colon);
        arg = filter.substring(colon + 1);
      }
      stream =
          switch (name) {
            case "pos" -> new KoreanPartOfSpeechStopFilter(stream, stopTags(arg));
            case "reading" -> new KoreanReadingFormFilter(stream);
            case "number" -> new KoreanNumberFilter(stream);
            case "lowercase" -> new LowerCaseFilter(stream);
            default -> throw new IllegalArgumentException("unknown filter: " + name);
          };
    }
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      tokenizer.setReader(new StringReader(input));
      out.println("# " + i);
      dumpStream(stream, input, out);
    }
  }

  /**
   * One line per token:
   *
   * <pre>
   * start byte, end byte, position increment, position length, token type (KNOWN/UNKNOWN/USER),
   * POS type, left POS, right POS, reading (or -), morphemes (surface/TAG+surface/TAG, or -), text
   * </pre>
   *
   * tab-separated; text and morpheme surfaces escaped. The token type comes from the underlying
   * ko.Token held by the POS attribute (it is not exposed through the attribute API).
   */
  private static void dumpStream(TokenStream stream, String input, PrintWriter out)
      throws Exception {
    CharTermAttribute term = stream.addAttribute(CharTermAttribute.class);
    OffsetAttribute offset = stream.addAttribute(OffsetAttribute.class);
    PositionIncrementAttribute posInc = stream.addAttribute(PositionIncrementAttribute.class);
    PositionLengthAttribute posLen = stream.addAttribute(PositionLengthAttribute.class);
    PartOfSpeechAttribute pos = stream.addAttribute(PartOfSpeechAttribute.class);
    ReadingAttribute reading = stream.addAttribute(ReadingAttribute.class);
    int[] byteAt = byteOffsets(input);
    stream.reset();
    while (stream.incrementToken()) {
      Token token = (Token) field(pos, "token");
      TokenType type = token == null ? null : token.getType();
      StringBuilder sb = new StringBuilder();
      sb.append(byteAt[offset.startOffset()]).append('\t');
      sb.append(byteAt[offset.endOffset()]).append('\t');
      sb.append(posInc.getPositionIncrement()).append('\t');
      sb.append(posLen.getPositionLength()).append('\t');
      sb.append(type == null ? "-" : type.name()).append('\t');
      sb.append(pos.getPOSType() == null ? "-" : pos.getPOSType().name()).append('\t');
      sb.append(pos.getLeftPOS() == null ? "-" : pos.getLeftPOS().name()).append('\t');
      sb.append(pos.getRightPOS() == null ? "-" : pos.getRightPOS().name()).append('\t');
      sb.append(reading.getReading() == null ? "-" : escape(reading.getReading())).append('\t');
      sb.append(morphemes(pos.getMorphemes())).append('\t');
      sb.append(escape(term.toString()));
      out.println(sb);
    }
    stream.end();
    stream.close();
  }

  private static String morphemes(KoMorphData.Morpheme[] morphemes) {
    if (morphemes == null) return "-";
    StringBuilder sb = new StringBuilder();
    for (KoMorphData.Morpheme m : morphemes) {
      if (sb.length() > 0) sb.append('+');
      sb.append(escape(m.surfaceForm())).append('/').append(m.posTag().name());
    }
    return sb.toString();
  }

  // ---------------------------------------------------------------------------------------------
  // chardef

  private static final String[] CLASS_NAMES = {
    "NGRAM", "DEFAULT", "SPACE", "SYMBOL", "NUMERIC", "ALPHA", "CYRILLIC", "GREEK", "HIRAGANA",
    "KATAKANA", "KANJI", "HANGUL", "HANJA", "HANJANUMERIC"
  };

  /**
   * First one {@code class\t<name>\t<invoke 0/1>\t<group 0/1>} line per character class (in
   * ordinal order), then one {@code <first hex>\t<last hex>\t<class name>} line per run of code
   * units with the same class.
   */
  private static void charDef(PrintWriter out) throws Exception {
    CharacterDefinition def = CharacterDefinition.getInstance();
    boolean[] invoke = (boolean[]) field(def, "invokeMap");
    boolean[] group = (boolean[]) field(def, "groupMap");
    if (invoke.length != CLASS_NAMES.length) throw new IllegalStateException("class count");
    for (int i = 0; i < CLASS_NAMES.length; i++) {
      out.println(
          "class\t" + CLASS_NAMES[i] + "\t" + (invoke[i] ? 1 : 0) + "\t" + (group[i] ? 1 : 0));
    }
    int runStart = 0;
    int runClass = def.getCharacterClass((char) 0);
    for (int c = 1; c <= 0x10000; c++) {
      int cls = c == 0x10000 ? -1 : def.getCharacterClass((char) c);
      if (cls != runClass) {
        out.printf("%04X\t%04X\t%s%n", runStart, c - 1, CLASS_NAMES[runClass]);
        runStart = c;
        runClass = cls;
      }
    }
  }

  // ---------------------------------------------------------------------------------------------
  // unicode / lowercase

  /**
   * The JDK character properties nori's Viterbi consults on UTF-16 code units: one {@code <first
   * hex>\t<last hex>\t<Character.getType>\t<UnicodeScript name>\t<isDigit 0/1>} line per run of
   * code units with identical values.
   */
  private static void unicode(PrintWriter out) {
    int runStart = 0;
    String runValue = unicodeValue(0);
    for (int c = 1; c <= 0x10000; c++) {
      String value = c == 0x10000 ? null : unicodeValue(c);
      if (!runValue.equals(value)) {
        out.printf("%04X\t%04X\t%s%n", runStart, c - 1, runValue);
        runStart = c;
        runValue = value;
      }
    }
  }

  private static String unicodeValue(int c) {
    return Character.getType(c)
        + "\t"
        + Character.UnicodeScript.of(c).name()
        + "\t"
        + (Character.isDigit(c) ? 1 : 0);
  }

  /**
   * What Lucene's LowerCaseFilter does per code point: one {@code <hex>\t<hex>} line for every
   * code point that Character.toLowerCase(int) maps to something else.
   */
  private static void lowercase(PrintWriter out) {
    for (int cp = 0; cp <= 0x10FFFF; cp++) {
      int lower = Character.toLowerCase(cp);
      if (lower != cp) out.printf("%04X\t%04X%n", cp, lower);
    }
  }

  // ---------------------------------------------------------------------------------------------
  // dict

  /**
   * Header lines summarise the dictionaries in an implementation-independent way so the Rust port
   * can check its converted copy is complete, then one block per probe:
   *
   * <pre>
   * tokeninfo.terms     &lt;FST term count&gt;
   * tokeninfo.words     &lt;word (entry) count&gt;
   * tokeninfo.checksum  &lt;fnv1a-64 over every canonical word line, in FST order then word-id order&gt;
   * unk  &lt;class&gt; &lt;leftId&gt; &lt;rightId&gt; &lt;wordCost&gt; &lt;leftPOS&gt;        (one per unknown entry, class order)
   * costs.forward &lt;n&gt;  costs.backward &lt;n&gt;  costs.checksum &lt;fnv1a-64 over i16 LE, forward-major&gt;
   * term  &lt;surface&gt; &lt;word count&gt;                           (probe "term X": X looked up as a whole)
   * word  &lt;surface&gt; &lt;i&gt; &lt;leftId&gt; &lt;rightId&gt; &lt;cost&gt; &lt;posType&gt; &lt;leftPOS&gt; &lt;rightPOS&gt; &lt;reading&gt; &lt;morphemes&gt;
   * miss  &lt;surface&gt;                                        (probe term not in the dictionary)
   * prefix &lt;text&gt; &lt;surface&gt; ...                            (probe "prefix X": every dictionary term
   *                                                          that is a prefix of X, shortest first)
   * cost  &lt;forwardId&gt; &lt;backwardId&gt; &lt;value&gt;                    (probe "cost F B")
   * </pre>
   *
   * A canonical word line is {@code surface\tleftId\trightId\twordCost\tposType\tleftPOS\trightPOS
   * \treading\tmorphemes\n} (reading/morphemes as in the token dumps, "-" when absent), UTF-8.
   * With dump=&lt;dir&gt;, every canonical word line goes to {@code <dir>/tokeninfo.tsv}, the unknown
   * entries to {@code <dir>/unk.tsv} and the cost matrix to {@code <dir>/costs.bin} (i16 LE,
   * forward-major) for the Rust dictionary converter to consume.
   */
  private static void dict(List<String> probes, String dumpDir, PrintWriter out) throws Exception {
    TokenInfoDictionary tid = TokenInfoDictionary.getInstance();
    UnknownDictionary unk = UnknownDictionary.getInstance();
    ConnectionCosts costs = ConnectionCosts.getInstance();
    CharacterDefinition charDef = CharacterDefinition.getInstance();
    PrintWriter dump = dumpDir == null ? null : writer(Paths.get(dumpDir, "tokeninfo.tsv").toString());

    // --- token info dictionary: every term in FST order, every word of a term in word-id order.
    FST<Long> fst = (FST<Long>) invokePrivate(tid.getFST(), "getInternalFST");
    IntsRefFSTEnum<Long> fstEnum = new IntsRefFSTEnum<>(fst);
    IntsRefFSTEnum.InputOutput<Long> mapping;
    IntsRef scratch = new IntsRef();
    long checksum = FNV_OFFSET;
    int terms = 0;
    int words = 0;
    while ((mapping = fstEnum.next()) != null) {
      terms++;
      String surface = intsToString(mapping.input);
      tid.lookupWordIds(mapping.output.intValue(), scratch);
      for (int i = 0; i < scratch.length; i++) {
        words++;
        String line = wordLine(tid.getMorphAttributes(), surface, scratch.ints[scratch.offset + i]);
        checksum = fnv1a(checksum, line.getBytes(StandardCharsets.UTF_8));
        if (dump != null) dump.print(line);
      }
    }
    if (dump != null) dump.close();
    out.println("tokeninfo.terms\t" + terms);
    out.println("tokeninfo.words\t" + words);
    out.printf("tokeninfo.checksum\t%016x%n", checksum);

    // --- unknown dictionary: one entry per character class (looked up by class id).
    PrintWriter unkDump = dumpDir == null ? null : writer(Paths.get(dumpDir, "unk.tsv").toString());
    for (int cls = 0; cls < CLASS_NAMES.length; cls++) {
      unk.lookupWordIds(cls, scratch);
      for (int i = 0; i < scratch.length; i++) {
        int wordId = scratch.ints[scratch.offset + i];
        KoMorphData m = unk.getMorphAttributes();
        String line =
            CLASS_NAMES[cls]
                + "\t"
                + m.getLeftId(wordId)
                + "\t"
                + m.getRightId(wordId)
                + "\t"
                + m.getWordCost(wordId)
                + "\t"
                + m.getLeftPOS(wordId).name();
        out.println("unk\t" + line);
        if (unkDump != null) unkDump.println(line);
      }
    }
    if (unkDump != null) unkDump.close();

    // --- connection costs: the whole matrix.
    int forward = (int) field(costs, "forwardSize");
    java.nio.ByteBuffer costBuffer = (java.nio.ByteBuffer) field(costs, "buffer");
    int backward = costBuffer.limit() / 2 / forward;
    long costChecksum = FNV_OFFSET;
    DataOutputStream costDump =
        dumpDir == null
            ? null
            : new DataOutputStream(
                new java.io.BufferedOutputStream(
                    new FileOutputStream(Paths.get(dumpDir, "costs.bin").toFile())));
    byte[] two = new byte[2];
    for (int f = 0; f < forward; f++) {
      for (int b = 0; b < backward; b++) {
        int v = costs.get(f, b);
        two[0] = (byte) v;
        two[1] = (byte) (v >> 8);
        costChecksum = fnv1a(costChecksum, two);
        if (costDump != null) costDump.write(two);
      }
    }
    if (costDump != null) costDump.close();
    out.println("costs.forward\t" + forward);
    out.println("costs.backward\t" + backward);
    out.printf("costs.checksum\t%016x%n", costChecksum);

    // --- probes.
    for (String probe : probes) {
      if (probe.isEmpty() || probe.startsWith("//")) continue;
      int space = probe.indexOf(' ');
      String kind = probe.substring(0, space);
      String arg = probe.substring(space + 1);
      switch (kind) {
        case "term" -> {
          Long output = lookupExact(fst, arg);
          if (output == null) {
            out.println("miss\t" + escape(arg));
          } else {
            tid.lookupWordIds(output.intValue(), scratch);
            out.println("term\t" + escape(arg) + "\t" + scratch.length);
            for (int i = 0; i < scratch.length; i++) {
              String line =
                  wordLine(tid.getMorphAttributes(), arg, scratch.ints[scratch.offset + i]);
              // wordLine starts with the surface; replace it with the probe index.
              out.println("word\t" + escape(arg) + "\t" + i + "\t" + line.substring(line.indexOf('\t') + 1).stripTrailing());
            }
          }
        }
        case "prefix" -> {
          StringBuilder sb = new StringBuilder("prefix\t" + escape(arg));
          for (int len = 1; len <= arg.length(); len++) {
            if (lookupExact(fst, arg.substring(0, len)) != null) {
              sb.append('\t').append(escape(arg.substring(0, len)));
            }
          }
          out.println(sb);
        }
        case "cost" -> {
          String[] ids = arg.split(" ");
          int f = Integer.parseInt(ids[0]);
          int b = Integer.parseInt(ids[1]);
          out.println("cost\t" + f + "\t" + b + "\t" + costs.get(f, b));
        }
        default -> throw new IllegalArgumentException("unknown probe: " + probe);
      }
    }
  }

  private static String wordLine(KoMorphData m, String surface, int wordId) {
    char[] chars = surface.toCharArray();
    String reading = m.getReading(wordId);
    return escape(surface)
        + "\t"
        + m.getLeftId(wordId)
        + "\t"
        + m.getRightId(wordId)
        + "\t"
        + m.getWordCost(wordId)
        + "\t"
        + m.getPOSType(wordId).name()
        + "\t"
        + m.getLeftPOS(wordId).name()
        + "\t"
        + m.getRightPOS(wordId).name()
        + "\t"
        + (reading == null ? "-" : escape(reading))
        + "\t"
        + morphemes(m.getMorphemes(wordId, chars, 0, chars.length))
        + "\n";
  }

  /** Exact lookup of a UTF-16 string in a PositiveIntOutputs FST; null if absent. */
  private static Long lookupExact(FST<Long> fst, String s) throws IOException {
    FST.BytesReader reader = fst.getBytesReader();
    FST.Arc<Long> arc = fst.getFirstArc(new FST.Arc<>());
    long output = 0;
    for (int i = 0; i < s.length(); i++) {
      if (fst.findTargetArc(s.charAt(i), arc, arc, reader) == null) return null;
      output += arc.output();
    }
    if (!arc.isFinal()) return null;
    return output + arc.nextFinalOutput();
  }

  private static String intsToString(IntsRef ints) {
    char[] chars = new char[ints.length];
    for (int i = 0; i < chars.length; i++) chars[i] = (char) ints.ints[ints.offset + i];
    return new String(chars);
  }

  // ---------------------------------------------------------------------------------------------
  // userdict

  /**
   * What {@code UserDictionary.open} builds from a rules file: {@code empty} when it returns null,
   * {@code error\t<message>} when it throws, else one {@code entry\t<surface>\t<ord>\t<rightId>\t
   * <segmentation>} line per FST term in FST order, segmentation being the morpheme surfaces joined
   * with '+' or "-" for a simple noun.
   */
  private static void userDict(String rulesPath, PrintWriter out) throws Exception {
    UserDictionary dict;
    try (Reader reader = Files.newBufferedReader(Paths.get(rulesPath), StandardCharsets.UTF_8)) {
      dict = UserDictionary.open(reader);
    } catch (IllegalArgumentException e) {
      out.println("error\t" + escape(e.getMessage()));
      return;
    }
    if (dict == null) {
      out.println("empty");
      return;
    }
    FST<Long> fst = (FST<Long>) invokePrivate(dict.getFST(), "getInternalFST");
    IntsRefFSTEnum<Long> fstEnum = new IntsRefFSTEnum<>(fst);
    IntsRefFSTEnum.InputOutput<Long> mapping;
    while ((mapping = fstEnum.next()) != null) {
      String surface = intsToString(mapping.input);
      int ord = mapping.output.intValue();
      KoMorphData m = dict.getMorphAttributes();
      char[] chars = surface.toCharArray();
      KoMorphData.Morpheme[] morphemes = m.getMorphemes(ord, chars, 0, chars.length);
      StringBuilder seg = new StringBuilder();
      if (morphemes == null) {
        seg.append('-');
      } else {
        for (KoMorphData.Morpheme morpheme : morphemes) {
          if (seg.length() > 0) seg.append('+');
          seg.append(escape(morpheme.surfaceForm()));
        }
      }
      out.println("entry\t" + escape(surface) + "\t" + ord + "\t" + m.getRightId(ord) + "\t" + seg);
    }
  }

  // ---------------------------------------------------------------------------------------------
  // Reflection helpers (for internals the public API doesn't expose).

  private static Object field(Object instance, String name) throws Exception {
    Class<?> cls = instance.getClass();
    while (cls != null) {
      try {
        Field f = cls.getDeclaredField(name);
        f.setAccessible(true);
        return f.get(instance);
      } catch (NoSuchFieldException e) {
        cls = cls.getSuperclass();
      }
    }
    throw new NoSuchFieldException(name);
  }

  private static Object invokePrivate(Object instance, String name) throws Exception {
    var m = instance.getClass().getDeclaredMethod(name);
    m.setAccessible(true);
    return m.invoke(instance);
  }

  private static final long FNV_OFFSET = 0xcbf29ce484222325L;
  private static final long FNV_PRIME = 0x100000001b3L;

  private static long fnv1a(long hash, byte[] bytes) {
    for (byte b : bytes) {
      hash ^= (b & 0xFF);
      hash *= FNV_PRIME;
    }
    return hash;
  }

  // ---------------------------------------------------------------------------------------------
  // I/O helpers (same conventions as the smartcn generator).

  /** Byte offset (in UTF-8) of every UTF-16 index of {@code s}, including {@code s.length()}. */
  private static int[] byteOffsets(String s) {
    int[] byteAt = new int[s.length() + 1];
    int bytes = 0;
    for (int i = 0; i < s.length(); i++) {
      byteAt[i] = bytes;
      char c = s.charAt(i);
      if (c < 0x80) bytes += 1;
      else if (c < 0x800) bytes += 2;
      else if (Character.isHighSurrogate(c)) bytes += 4;
      else if (Character.isLowSurrogate(c)) bytes += 0; // counted on the high surrogate
      else bytes += 3;
    }
    byteAt[s.length()] = bytes;
    return byteAt;
  }

  private static List<String> readCases(String path) throws IOException {
    List<String> inputs = new ArrayList<>();
    try (BufferedReader reader = Files.newBufferedReader(Paths.get(path), StandardCharsets.UTF_8)) {
      String line;
      while ((line = reader.readLine()) != null) inputs.add(unescape(line));
    }
    return inputs;
  }

  private static PrintWriter writer(String path) throws IOException {
    Path p = Paths.get(path);
    if (p.getParent() != null) Files.createDirectories(p.getParent());
    BufferedWriter w = Files.newBufferedWriter(p, StandardCharsets.UTF_8);
    return new PrintWriter(w) {
      @Override
      public void println() {
        write('\n'); // never platform line endings
      }
    };
  }

  /** Inverse of {@link #escape}. */
  static String unescape(String s) {
    StringBuilder sb = new StringBuilder(s.length());
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      if (c != '\\') {
        sb.append(c);
        continue;
      }
      char e = s.charAt(++i);
      switch (e) {
        case '\\' -> sb.append('\\');
        case 'n' -> sb.append('\n');
        case 'r' -> sb.append('\r');
        case 't' -> sb.append('\t');
        case 'u' -> {
          if (s.charAt(++i) != '{') throw new IllegalArgumentException("bad escape in: " + s);
          int close = s.indexOf('}', i);
          sb.appendCodePoint(Integer.parseInt(s.substring(i + 1, close), 16));
          i = close;
        }
        default -> throw new IllegalArgumentException("bad escape in: " + s);
      }
    }
    return sb.toString();
  }

  /**
   * Escapes backslash, CR, LF, TAB, other C0/C1 controls, DEL and U+2028/U+2029. Lone surrogates
   * (which the tokenizer produces when an unknown run is cut at 1024 code units inside a pair) are
   * escaped as their code unit since they can't be encoded in UTF-8; the Rust side recognises
   * those.
   */
  static String escape(String s) {
    StringBuilder sb = new StringBuilder(s.length());
    s.codePoints()
        .forEach(
            cp -> {
              switch (cp) {
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> {
                  boolean loneSurrogate = cp >= 0xD800 && cp <= 0xDFFF;
                  if (loneSurrogate
                      || cp < 0x20
                      || (cp >= 0x7F && cp < 0xA0)
                      || cp == 0x2028
                      || cp == 0x2029) {
                    sb.append("\\u{").append(Integer.toHexString(cp)).append('}');
                  } else {
                    sb.appendCodePoint(cp);
                  }
                }
              }
            });
    return sb.toString();
  }
}
