// Golden-file generator for the Rust port of Lucene's kuromoji (Japanese) analyzer.
//
// Runs the reference Java implementation (lucene-analysis-kuromoji, as used by Elasticsearch's
// analysis-kuromoji plugin) over a set of inputs and writes out exactly what it produced, so the
// Rust port can be checked against it without a JVM at test time. See ../README.md for the file
// formats and ../gen.sh for how this is compiled and run.
//
// Modes (options are key=value pairs, see the comments at each section):
//   tokens     <cases> <out> [opts]   JapaneseTokenizer output (ES `kuromoji_tokenizer`)
//   analyze    <cases> <out> [opts]   JapaneseAnalyzer output (ES `kuromoji` analyzer)
//   completion <cases> <out> [opts]   JapaneseCompletionAnalyzer output (ES `kuromoji_completion`)
//   chain      <cases> <out> filters=<spec> [opts]   (char filter +) tokenizer + a filter chain
//   charfilter <cases> <out> charfilter=<spec>      a char filter's text and offset map
//   romaji     <cases> <out>          ToStringUtil.getRomanization and KatakanaRomanizer per line
//   number     <cases> <out>          JapaneseNumberFilter.normalizeNumber per line
//   chardef    <out>                  CharacterDefinition class of every UTF-16 code unit + flags
//   unicode    <out>                  JDK Character.getType per code unit
//   lowercase  <out>                  Character.toLowerCase(int) for every code point it changes
//   stoplists  <out>                  JapaneseAnalyzer's default stop tags and stop words
//   dict       <probes> <out> [dump=<dir>]   dictionary checksums + probe lookups (+ full dump)
//   userdict   <rules> <out>          what UserDictionary.open builds from a rules file

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
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;
import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.analysis.CharArraySet;
import org.apache.lucene.analysis.CharFilter;
import org.apache.lucene.analysis.LowerCaseFilter;
import org.apache.lucene.analysis.StopFilter;
import org.apache.lucene.analysis.TokenStream;
import org.apache.lucene.analysis.cjk.CJKWidthCharFilter;
import org.apache.lucene.analysis.cjk.CJKWidthFilter;
import org.apache.lucene.analysis.core.KeywordTokenizer;
import org.apache.lucene.analysis.ja.JapaneseAnalyzer;
import org.apache.lucene.analysis.ja.JapaneseBaseFormFilter;
import org.apache.lucene.analysis.ja.JapaneseCompletionAnalyzer;
import org.apache.lucene.analysis.ja.JapaneseCompletionFilter;
import org.apache.lucene.analysis.ja.JapaneseHiraganaUppercaseFilter;
import org.apache.lucene.analysis.ja.JapaneseIterationMarkCharFilter;
import org.apache.lucene.analysis.ja.JapaneseKatakanaStemFilter;
import org.apache.lucene.analysis.ja.JapaneseKatakanaUppercaseFilter;
import org.apache.lucene.analysis.ja.JapaneseNumberFilter;
import org.apache.lucene.analysis.ja.JapanesePartOfSpeechStopFilter;
import org.apache.lucene.analysis.ja.JapaneseReadingFormFilter;
import org.apache.lucene.analysis.ja.JapaneseTokenizer;
import org.apache.lucene.analysis.ja.Token;
import org.apache.lucene.analysis.ja.completion.CharSequenceUtils;
import org.apache.lucene.analysis.ja.completion.KatakanaRomanizer;
import org.apache.lucene.analysis.ja.dict.CharacterDefinition;
import org.apache.lucene.analysis.ja.dict.ConnectionCosts;
import org.apache.lucene.analysis.ja.dict.JaMorphData;
import org.apache.lucene.analysis.ja.dict.ToStringUtil;
import org.apache.lucene.analysis.ja.dict.TokenInfoDictionary;
import org.apache.lucene.analysis.ja.dict.UnknownDictionary;
import org.apache.lucene.analysis.ja.dict.UserDictionary;
import org.apache.lucene.analysis.ja.tokenattributes.BaseFormAttribute;
import org.apache.lucene.analysis.ja.tokenattributes.InflectionAttribute;
import org.apache.lucene.analysis.ja.tokenattributes.PartOfSpeechAttribute;
import org.apache.lucene.analysis.ja.tokenattributes.ReadingAttribute;
import org.apache.lucene.analysis.morph.TokenType;
import org.apache.lucene.analysis.tokenattributes.CharTermAttribute;
import org.apache.lucene.analysis.tokenattributes.OffsetAttribute;
import org.apache.lucene.analysis.tokenattributes.PositionIncrementAttribute;
import org.apache.lucene.analysis.tokenattributes.PositionLengthAttribute;
import org.apache.lucene.util.CharsRef;
import org.apache.lucene.util.IntsRef;
import org.apache.lucene.util.fst.FST;
import org.apache.lucene.util.fst.IntsRefFSTEnum;

@SuppressWarnings("unchecked")
public final class KuromojiGolden {
  /** Written for a null attribute; the escaper never produces it. */
  private static final String NULL = "\\N";

  public static void main(String[] args) throws Exception {
    String mode = args[0];
    switch (mode) {
      case "tokens", "analyze", "completion", "chain", "charfilter", "romaji", "number" -> {
        List<String> inputs = readCases(args[1]);
        Map<String, String> opts = parseOptions(args, 3);
        try (PrintWriter out = writer(args[2])) {
          switch (mode) {
            case "tokens" -> tokens(inputs, opts, out);
            case "analyze" -> analyze(inputs, opts, out);
            case "completion" -> completion(inputs, opts, out);
            case "chain" -> chain(inputs, opts, out);
            case "charfilter" -> charFilterText(inputs, opts, out);
            case "romaji" -> romaji(inputs, out);
            default -> number(inputs, out);
          }
        }
      }
      case "chardef", "unicode", "lowercase", "stoplists" -> {
        try (PrintWriter out = writer(args[1])) {
          switch (mode) {
            case "chardef" -> charDef(out);
            case "unicode" -> unicode(out);
            case "lowercase" -> lowercase(out);
            default -> stopLists(out);
          }
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
  //   mode=normal|search|extended     (default search, like ES)
  //   punctuation=keep|discard        (default discard, like ES)
  //   compound=keep|discard           (default keep, like ES's kuromoji_tokenizer)
  //   nbest=<int>                     (default -1: off, like ES)
  //   nbest_examples=<str>            (ES nbest_examples; the larger of the two costs applies)
  //   userdict=<rules file>           (default none)
  //   charfilter=<spec>               (tokens/chain only; itermark[:kanji=0|1,kana=0|1] or width)
  //   stopwords=<file>  stopwords_case=true|false   (analyze only; default: Lucene's Japanese list)
  //   completion=index|query          (completion only; default index)
  //   filters=<spec>                  (chain only; comma-separated: baseform, pos[:TAG+TAG],
  //                                    stop[:file[:nocase]], stem[:min], reading, romaji, number,
  //                                    hiragana_upper, katakana_upper, width, completion[:mode],
  //                                    lowercase; applied in order)

  private static JapaneseTokenizer.Mode mode(Map<String, String> opts) {
    return JapaneseTokenizer.Mode.valueOf(
        opts.getOrDefault("mode", "search").toUpperCase(Locale.ROOT));
  }

  private static UserDictionary userDictionary(Map<String, String> opts) throws IOException {
    String path = opts.get("userdict");
    if (path == null) return null;
    try (Reader reader = Files.newBufferedReader(Paths.get(path), StandardCharsets.UTF_8)) {
      return UserDictionary.open(reader);
    }
  }

  /** Same construction as Elasticsearch's KuromojiTokenizerFactory.create. */
  private static JapaneseTokenizer newTokenizer(Map<String, String> opts) throws IOException {
    JapaneseTokenizer t =
        new JapaneseTokenizer(
            TokenStream.DEFAULT_TOKEN_ATTRIBUTE_FACTORY,
            userDictionary(opts),
            !opts.getOrDefault("punctuation", "discard").equals("keep"),
            opts.getOrDefault("compound", "keep").equals("discard"),
            mode(opts));
    int nBestCost = Integer.parseInt(opts.getOrDefault("nbest", "-1"));
    String examples = opts.get("nbest_examples");
    if (examples != null) {
      nBestCost = Math.max(nBestCost, t.calcNBestCost(examples));
    }
    t.setNBestCost(nBestCost);
    return t;
  }

  private static CharFilter charFilter(String spec, Reader reader) {
    String name = spec;
    boolean kanji = true;
    boolean kana = true;
    int colon = spec.indexOf(':');
    if (colon >= 0) {
      name = spec.substring(0, colon);
      for (String flag : spec.substring(colon + 1).split(",")) {
        String[] kv = flag.split("=");
        boolean value = kv[1].equals("1") || kv[1].equals("true");
        switch (kv[0]) {
          case "kanji" -> kanji = value;
          case "kana" -> kana = value;
          default -> throw new IllegalArgumentException("unknown char filter flag: " + flag);
        }
      }
    }
    return switch (name) {
      case "itermark" -> new JapaneseIterationMarkCharFilter(reader, kanji, kana);
      case "width" -> new CJKWidthCharFilter(reader);
      default -> throw new IllegalArgumentException("unknown char filter: " + name);
    };
  }

  private static Reader input(Map<String, String> opts, String text) {
    Reader reader = new StringReader(text);
    String spec = opts.get("charfilter");
    return spec == null ? reader : charFilter(spec, reader);
  }

  private static void tokens(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    JapaneseTokenizer tokenizer = newTokenizer(opts);
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      tokenizer.setReader(input(opts, input));
      out.println("# " + i);
      dumpStream(tokenizer, input, out);
    }
  }

  /** Lines of a word list file, minus comments and blank lines. */
  private static List<String> wordList(String path) throws IOException {
    List<String> words = new ArrayList<>();
    for (String line : Files.readAllLines(Paths.get(path), StandardCharsets.UTF_8)) {
      if (line.isEmpty() || line.startsWith("#")) continue;
      words.add(line);
    }
    return words;
  }

  /** Elasticsearch's stop-word resolution for the kuromoji analyzer (no named lists needed). */
  private static CharArraySet stopWords(Map<String, String> opts) throws IOException {
    String path = opts.get("stopwords");
    if (path == null) return JapaneseAnalyzer.getDefaultStopSet();
    boolean ignoreCase = Boolean.parseBoolean(opts.getOrDefault("stopwords_case", "false"));
    return new CharArraySet(wordList(path), ignoreCase);
  }

  private static void analyze(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    // Same construction as Elasticsearch's KuromojiAnalyzerProvider.
    try (Analyzer analyzer =
        new JapaneseAnalyzer(
            userDictionary(opts),
            mode(opts),
            CharArraySet.copy(stopWords(opts)),
            JapaneseAnalyzer.getDefaultStopTags())) {
      for (int i = 0; i < inputs.size(); i++) {
        String input = inputs.get(i);
        out.println("# " + i);
        dumpStream(analyzer.tokenStream("", input), input, out);
      }
    }
  }

  private static JapaneseCompletionFilter.Mode completionMode(String spec) {
    return spec != null && spec.equalsIgnoreCase("query")
        ? JapaneseCompletionFilter.Mode.QUERY
        : JapaneseCompletionFilter.Mode.INDEX;
  }

  private static void completion(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    // Same construction as Elasticsearch's KuromojiCompletionAnalyzerProvider.
    try (Analyzer analyzer =
        new JapaneseCompletionAnalyzer(userDictionary(opts), completionMode(opts.get("completion")))) {
      for (int i = 0; i < inputs.size(); i++) {
        String input = inputs.get(i);
        out.println("# " + i);
        dumpStream(analyzer.tokenStream("", input), input, out);
      }
    }
  }

  private static Set<String> stopTags(String spec) {
    if (spec == null) return JapaneseAnalyzer.getDefaultStopTags();
    return new HashSet<>(Arrays.asList(spec.split("\\+")));
  }

  private static void chain(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    String spec = opts.get("filters");
    if (spec == null) throw new IllegalArgumentException("chain needs filters=...");
    JapaneseTokenizer tokenizer = newTokenizer(opts);
    TokenStream stream = tokenizer;
    for (String filter : spec.isEmpty() ? new String[0] : spec.split(",")) {
      String[] parts = filter.split(":");
      String name = parts[0];
      String arg = parts.length > 1 ? parts[1] : null;
      stream =
          switch (name) {
            case "baseform" -> new JapaneseBaseFormFilter(stream);
            case "pos" -> new JapanesePartOfSpeechStopFilter(stream, stopTags(arg));
            case "stop" -> {
              CharArraySet words =
                  arg == null
                      ? JapaneseAnalyzer.getDefaultStopSet()
                      : new CharArraySet(wordList(arg), parts.length > 2 && parts[2].equals("nocase"));
              yield new StopFilter(stream, words);
            }
            case "stem" -> arg == null
                ? new JapaneseKatakanaStemFilter(stream)
                : new JapaneseKatakanaStemFilter(stream, Integer.parseInt(arg));
            case "reading" -> new JapaneseReadingFormFilter(stream, false);
            case "romaji" -> new JapaneseReadingFormFilter(stream, true);
            case "number" -> new JapaneseNumberFilter(stream);
            case "hiragana_upper" -> new JapaneseHiraganaUppercaseFilter(stream);
            case "katakana_upper" -> new JapaneseKatakanaUppercaseFilter(stream);
            case "width" -> new CJKWidthFilter(stream);
            case "completion" -> new JapaneseCompletionFilter(stream, completionMode(arg));
            case "lowercase" -> new LowerCaseFilter(stream);
            default -> throw new IllegalArgumentException("unknown filter: " + name);
          };
    }
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      tokenizer.setReader(input(opts, input));
      out.println("# " + i);
      dumpStream(stream, input, out);
    }
  }

  /**
   * One line per token:
   *
   * <pre>
   * start byte, end byte, position increment, position length, token type (KNOWN/UNKNOWN/USER),
   * part of speech, base form, reading, pronunciation, inflection type, inflection form, text
   * </pre>
   *
   * tab-separated; strings escaped, nulls as {@code \N}. Offsets are UTF-8 byte offsets into the
   * original (pre-char-filter) input. The token type comes from the underlying ja.Token held by
   * the POS attribute (it is not exposed through the attribute API); it is null after a filter
   * that clears attributes (the completion filter).
   */
  private static void dumpStream(TokenStream stream, String input, PrintWriter out)
      throws Exception {
    CharTermAttribute term = stream.addAttribute(CharTermAttribute.class);
    OffsetAttribute offset = stream.addAttribute(OffsetAttribute.class);
    PositionIncrementAttribute posInc = stream.addAttribute(PositionIncrementAttribute.class);
    PositionLengthAttribute posLen = stream.addAttribute(PositionLengthAttribute.class);
    PartOfSpeechAttribute pos = stream.addAttribute(PartOfSpeechAttribute.class);
    BaseFormAttribute baseForm = stream.addAttribute(BaseFormAttribute.class);
    ReadingAttribute reading = stream.addAttribute(ReadingAttribute.class);
    InflectionAttribute inflection = stream.addAttribute(InflectionAttribute.class);
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
      sb.append(type == null ? NULL : type.name()).append('\t');
      sb.append(nullable(pos.getPartOfSpeech())).append('\t');
      sb.append(nullable(baseForm.getBaseForm())).append('\t');
      sb.append(nullable(reading.getReading())).append('\t');
      sb.append(nullable(reading.getPronunciation())).append('\t');
      sb.append(nullable(inflection.getInflectionType())).append('\t');
      sb.append(nullable(inflection.getInflectionForm())).append('\t');
      sb.append(escape(term.toString()));
      out.println(sb);
    }
    stream.end();
    stream.close();
  }

  private static String nullable(String s) {
    return s == null ? NULL : escape(s);
  }

  // ---------------------------------------------------------------------------------------------
  // charfilter: the filtered text and the offset map of a char filter.

  /**
   * Per input: {@code # <i>}, {@code text\t<escaped filtered text>}, and {@code map\t<fb>:<ob>
   * ...} giving the original byte offset ({@code correctOffset}) of every code-point boundary of
   * the filtered text (as a byte offset into it), including the end.
   */
  private static void charFilterText(List<String> inputs, Map<String, String> opts, PrintWriter out)
      throws Exception {
    String spec = opts.get("charfilter");
    if (spec == null) throw new IllegalArgumentException("charfilter needs charfilter=...");
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      CharFilter filter = charFilter(spec, new StringReader(input));
      StringBuilder filtered = new StringBuilder();
      char[] buf = new char[1024];
      int n;
      while ((n = filter.read(buf, 0, buf.length)) != -1) filtered.append(buf, 0, n);
      filter.close();
      String text = filtered.toString();
      int[] filteredBytes = byteOffsets(text);
      int[] originalBytes = byteOffsets(input);
      out.println("# " + i);
      out.println("text\t" + escape(text));
      StringBuilder map = new StringBuilder("map");
      for (int o = 0; o <= text.length(); o++) {
        if (o < text.length() && Character.isLowSurrogate(text.charAt(o))) continue;
        map.append('\t')
            .append(filteredBytes[o])
            .append(':')
            .append(originalBytes[filter.correctOffset(o)]);
      }
      out.println(map);
    }
  }

  // ---------------------------------------------------------------------------------------------
  // romaji / number

  /**
   * Per input line: {@code hepburn\t<ToStringUtil.getRomanization>} and {@code
   * keystrokes\t<n>\t<k1>...} (KatakanaRomanizer.romanize) or {@code keystrokes\t!} when the input
   * isn't katakana plus ASCII lowercase (the romanizer's precondition).
   */
  private static void romaji(List<String> inputs, PrintWriter out) {
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      out.println("# " + i);
      out.println("hepburn\t" + escape(ToStringUtil.getRomanization(input)));
      if (CharSequenceUtils.isKatakanaOrHWAlphabets(input)) {
        List<CharsRef> keystrokes = KatakanaRomanizer.getInstance().romanize(new CharsRef(input));
        StringBuilder sb = new StringBuilder("keystrokes\t" + keystrokes.size());
        for (CharsRef k : keystrokes) sb.append('\t').append(escape(k.toString()));
        out.println(sb);
      } else {
        out.println("keystrokes\t!");
      }
    }
  }

  /** Per input line: {@code JapaneseNumberFilter.normalizeNumber(line)}, escaped. */
  private static void number(List<String> inputs, PrintWriter out) throws IOException {
    JapaneseNumberFilter filter = new JapaneseNumberFilter(new KeywordTokenizer());
    for (String input : inputs) {
      out.println(escape(filter.normalizeNumber(input)));
    }
    filter.close();
  }

  // ---------------------------------------------------------------------------------------------
  // chardef / unicode / lowercase / stoplists

  private static final String[] CLASS_NAMES = {
    "NGRAM", "DEFAULT", "SPACE", "SYMBOL", "NUMERIC", "ALPHA", "CYRILLIC", "GREEK", "HIRAGANA",
    "KATAKANA", "KANJI", "KANJINUMERIC"
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
      if (c < 0x10000 && def.isKanji((char) c) != (cls == 10 || cls == 11)) {
        throw new IllegalStateException("isKanji disagrees with the class at " + c);
      }
    }
  }

  /**
   * The JDK character property kuromoji's punctuation test consults on UTF-16 code units: one
   * {@code <first hex>\t<last hex>\t<Character.getType>} line per run of code units with the same
   * type.
   */
  private static void unicode(PrintWriter out) {
    int runStart = 0;
    int runType = Character.getType(0);
    for (int c = 1; c <= 0x10000; c++) {
      int type = c == 0x10000 ? -1 : Character.getType(c);
      if (type != runType) {
        out.printf("%04X\t%04X\t%d%n", runStart, c - 1, runType);
        runStart = c;
        runType = type;
      }
    }
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

  /** {@code tag\t<tag>} per default stop tag and {@code word\t<word>} per default stop word, sorted. */
  private static void stopLists(PrintWriter out) {
    for (String tag : new TreeSet<>(JapaneseAnalyzer.getDefaultStopTags())) {
      out.println("tag\t" + escape(tag));
    }
    TreeSet<String> words = new TreeSet<>();
    for (Object word : JapaneseAnalyzer.getDefaultStopSet()) {
      words.add(new String((char[]) word));
    }
    for (String word : words) out.println("word\t" + escape(word));
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
   * unk  &lt;class&gt; &lt;leftId&gt; &lt;rightId&gt; &lt;wordCost&gt; &lt;POS&gt;        (one per unknown entry, class order)
   * costs.forward &lt;n&gt;  costs.backward &lt;n&gt;  costs.checksum &lt;fnv1a-64 over i16 LE, forward-major&gt;
   * term  &lt;surface&gt; &lt;word count&gt;                           (probe "term X": X looked up as a whole)
   * word  &lt;surface&gt; &lt;i&gt; &lt;leftId&gt; &lt;rightId&gt; &lt;cost&gt; &lt;POS&gt; &lt;baseForm&gt; &lt;reading&gt; &lt;pronunciation&gt;
   *       &lt;inflType&gt; &lt;inflForm&gt;
   * miss  &lt;surface&gt;                                        (probe term not in the dictionary)
   * prefix &lt;text&gt; &lt;surface&gt; ...                            (probe "prefix X": every dictionary term
   *                                                          that is a prefix of X, shortest first)
   * cost  &lt;forwardId&gt; &lt;backwardId&gt; &lt;value&gt;                    (probe "cost F B")
   * </pre>
   *
   * A canonical word line is {@code surface\tleftId\trightId\twordCost\tPOS\tbaseForm\treading
   * \tpronunciation\tinflType\tinflForm\n} (strings escaped, nulls as \N), UTF-8. With
   * dump=&lt;dir&gt;, every canonical word line goes to {@code <dir>/tokeninfo.tsv}, the unknown
   * entries to {@code <dir>/unk.tsv} and the cost matrix to {@code <dir>/costs.bin} (i16 LE,
   * forward-major) for the Rust dictionary converter to consume.
   */
  private static void dict(List<String> probes, String dumpDir, PrintWriter out) throws Exception {
    TokenInfoDictionary tid = TokenInfoDictionary.getInstance();
    UnknownDictionary unk = UnknownDictionary.getInstance();
    ConnectionCosts costs = ConnectionCosts.getInstance();
    PrintWriter dump = dumpDir == null ? null : writer(Paths.get(dumpDir, "tokeninfo.tsv").toString());

    // --- token info dictionary: every term in FST order, every word of a term in word-id order.
    FST<Long> fst = (FST<Long>) invokePrivate(tid.getFST(), "getInternalFST");
    IntsRefFSTEnum<Long> fstEnum = new IntsRefFSTEnum<>(fst);
    IntsRefFSTEnum.InputOutput<Long> mapping;
    IntsRef scratch = new IntsRef();
    long checksum = FNV_OFFSET;
    int terms = 0;
    int words = 0;
    JaMorphData morph = tid.getMorphAttributes();
    while ((mapping = fstEnum.next()) != null) {
      terms++;
      String surface = intsToString(mapping.input);
      tid.lookupWordIds(mapping.output.intValue(), scratch);
      for (int i = 0; i < scratch.length; i++) {
        words++;
        String line = wordLine(morph, surface, scratch.ints[scratch.offset + i]);
        checksum = fnv1a(checksum, line.getBytes(StandardCharsets.UTF_8));
        if (dump != null) dump.print(line);
      }
    }
    if (dump != null) dump.close();
    out.println("tokeninfo.terms\t" + terms);
    out.println("tokeninfo.words\t" + words);
    out.printf("tokeninfo.checksum\t%016x%n", checksum);

    // --- unknown dictionary: entries per character class (looked up by class id).
    PrintWriter unkDump = dumpDir == null ? null : writer(Paths.get(dumpDir, "unk.tsv").toString());
    JaMorphData unkMorph = unk.getMorphAttributes();
    for (int cls = 0; cls < CLASS_NAMES.length; cls++) {
      unk.lookupWordIds(cls, scratch);
      for (int i = 0; i < scratch.length; i++) {
        int wordId = scratch.ints[scratch.offset + i];
        String line =
            CLASS_NAMES[cls]
                + "\t"
                + unkMorph.getLeftId(wordId)
                + "\t"
                + unkMorph.getRightId(wordId)
                + "\t"
                + unkMorph.getWordCost(wordId)
                + "\t"
                + escape(unkMorph.getPartOfSpeech(wordId));
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
              String line = wordLine(morph, arg, scratch.ints[scratch.offset + i]);
              // wordLine starts with the surface; replace it with the probe index.
              out.println(
                  "word\t"
                      + escape(arg)
                      + "\t"
                      + i
                      + "\t"
                      + line.substring(line.indexOf('\t') + 1).stripTrailing());
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

  private static String wordLine(JaMorphData m, String surface, int wordId) {
    char[] chars = surface.toCharArray();
    return escape(surface)
        + "\t"
        + m.getLeftId(wordId)
        + "\t"
        + m.getRightId(wordId)
        + "\t"
        + m.getWordCost(wordId)
        + "\t"
        + nullable(m.getPartOfSpeech(wordId))
        + "\t"
        + nullable(m.getBaseForm(wordId, chars, 0, chars.length))
        + "\t"
        + nullable(m.getReading(wordId, chars, 0, chars.length))
        + "\t"
        + nullable(m.getPronunciation(wordId, chars, 0, chars.length))
        + "\t"
        + nullable(m.getInflectionType(wordId))
        + "\t"
        + nullable(m.getInflectionForm(wordId))
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
   * {@code error\t<exception class>: <message>} when it throws, else one {@code entry\t<key>\t<ord>
   * \t<n>\t<segment 1>...<segment n>\t<reading 1>...<reading n>\t<POS>} line per FST term in FST
   * order. The key is the term-index key (the raw first CSV field); segments are the key cut at
   * the lengths Lucene recorded (which can leave part of the key uncovered, or take a space).
   */
  private static void userDict(String rulesPath, PrintWriter out) throws Exception {
    UserDictionary dict;
    try {
      dict = UserDictionary.open(new StringReader(elasticsearchRules(rulesPath)));
    } catch (Exception e) {
      out.println("error\t" + e.getClass().getSimpleName() + ": " + escape(String.valueOf(e.getMessage())));
      return;
    }
    if (dict == null) {
      out.println("empty");
      return;
    }
    FST<Long> fst = (FST<Long>) invokePrivate(dict.getFST(), "getInternalFST");
    IntsRefFSTEnum<Long> fstEnum = new IntsRefFSTEnum<>(fst);
    IntsRefFSTEnum.InputOutput<Long> mapping;
    JaMorphData m = dict.getMorphAttributes();
    while ((mapping = fstEnum.next()) != null) {
      String key = intsToString(mapping.input);
      int ord = mapping.output.intValue();
      int[] wordIdAndLength = dict.lookupSegmentation(ord);
      int wordId = wordIdAndLength[0];
      int n = wordIdAndLength.length - 1;
      StringBuilder sb = new StringBuilder("entry\t" + escape(key) + "\t" + ord + "\t" + n);
      int current = 0;
      for (int j = 1; j <= n; j++) {
        int len = wordIdAndLength[j];
        int start = Math.min(current, key.length());
        int end = Math.min(current + len, key.length());
        sb.append('\t').append(escape(key.substring(start, end)));
        current += len;
      }
      char[] chars = key.toCharArray();
      for (int j = 0; j < n; j++) {
        sb.append('\t').append(nullable(m.getReading(wordId + j, chars, 0, chars.length)));
      }
      sb.append('\t').append(nullable(m.getPartOfSpeech(wordId)));
      out.println(sb);
    }
  }

  /**
   * What Elasticsearch feeds Lucene for a {@code user_dictionary} file (Analysis.loadWordList +
   * deDuplicateRules with lenient=true, KuromojiTokenizerFactory.getUserDictionary): lines
   * trimmed, blank ones dropped, comments kept, a repeated surface form (CSV field 0) dropped,
   * joined with line separators. The strict duplicate error is checked natively by the Rust tests.
   */
  private static String elasticsearchRules(String rulesPath) throws IOException {
    StringBuilder sb = new StringBuilder();
    Set<String> seen = new HashSet<>();
    try (BufferedReader br = Files.newBufferedReader(Paths.get(rulesPath), StandardCharsets.UTF_8)) {
      String line;
      while ((line = br.readLine()) != null) {
        if (line.isBlank()) continue;
        line = line.trim();
        if (!line.startsWith("#")) {
          String[] values = org.apache.lucene.analysis.util.CSVUtil.parse(line);
          if (!seen.add(values[0])) continue;
        }
        sb.append(line).append(System.lineSeparator());
      }
    }
    return sb.toString();
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
    Class<?> cls = instance.getClass();
    while (cls != null) {
      try {
        var m = cls.getDeclaredMethod(name);
        m.setAccessible(true);
        return m.invoke(instance);
      } catch (NoSuchMethodException e) {
        cls = cls.getSuperclass();
      }
    }
    throw new NoSuchMethodException(name);
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
  // I/O helpers (same conventions as the smartcn and nori generators).

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
