// Golden-file generator for the Rust port of Lucene's smartcn analyzer.
//
// Runs the reference Java implementation (lucene-analysis-smartcn) over a set of inputs, writing
// out exactly what it produced, so the Rust port can be checked against it without a JVM at test
// time. See ../README.md for the file formats and ../gen.sh for how this is compiled and run.
//
// Modes:
//   tokens    <cases> <out>   HMMChineseTokenizer output (ES `smartcn_tokenizer`)
//   analyze   <cases> <out>   SmartChineseAnalyzer output (ES `smartcn` analyzer: + Porter + stop)
//   sentences <cases> <out>   JDK BreakIterator sentence boundaries (what the tokenizer splits on)
//   chartypes <out>           Utility.getCharType for every UTF-16 code unit, run-length encoded
//   dict      <probes> <out>  dictionary checksums + frequency lookups for a list of probe words

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.IOException;
import java.io.PrintWriter;
import java.io.StringReader;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.text.BreakIterator;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import org.apache.lucene.analysis.Analyzer;
import org.apache.lucene.analysis.TokenStream;
import org.apache.lucene.analysis.Tokenizer;
import org.apache.lucene.analysis.cn.smart.HMMChineseTokenizer;
import org.apache.lucene.analysis.cn.smart.SmartChineseAnalyzer;
import org.apache.lucene.analysis.cn.smart.Utility;
import org.apache.lucene.analysis.tokenattributes.CharTermAttribute;
import org.apache.lucene.analysis.tokenattributes.OffsetAttribute;
import org.apache.lucene.analysis.tokenattributes.PositionIncrementAttribute;

public final class SmartcnGolden {

  public static void main(String[] args) throws Exception {
    String mode = args[0];
    switch (mode) {
      case "tokens":
      case "analyze":
      case "sentences":
        {
          List<String> inputs = readCases(args[1]);
          try (PrintWriter out = writer(args[2])) {
            switch (mode) {
              case "tokens" -> tokens(inputs, out);
              case "analyze" -> analyze(inputs, out);
              default -> sentences(inputs, out);
            }
          }
          break;
        }
      case "chartypes":
        try (PrintWriter out = writer(args[1])) {
          charTypes(out);
        }
        break;
      case "dict":
        try (PrintWriter out = writer(args[2])) {
          dict(readCases(args[1]), out);
        }
        break;
      default:
        throw new IllegalArgumentException("unknown mode: " + mode);
    }
  }

  // ---------------------------------------------------------------------------------------------
  // tokens / analyze

  private static void tokens(List<String> inputs, PrintWriter out) throws IOException {
    Tokenizer tokenizer = new HMMChineseTokenizer();
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      tokenizer.setReader(new StringReader(input));
      out.println("# " + i);
      dumpStream(tokenizer, input, out);
    }
  }

  private static void analyze(List<String> inputs, PrintWriter out) throws IOException {
    // Same construction as Elasticsearch's SmartChineseAnalyzerProvider.
    try (Analyzer analyzer = new SmartChineseAnalyzer(SmartChineseAnalyzer.getDefaultStopSet())) {
      for (int i = 0; i < inputs.size(); i++) {
        String input = inputs.get(i);
        out.println("# " + i);
        dumpStream(analyzer.tokenStream("", input), input, out);
      }
    }
  }

  /** One line per token: {@code <start byte>\t<end byte>\t<position increment>\t<escaped text>}. */
  private static void dumpStream(TokenStream stream, String input, PrintWriter out)
      throws IOException {
    CharTermAttribute term = stream.addAttribute(CharTermAttribute.class);
    OffsetAttribute offset = stream.addAttribute(OffsetAttribute.class);
    PositionIncrementAttribute posInc = stream.addAttribute(PositionIncrementAttribute.class);
    int[] byteAt = byteOffsets(input);
    stream.reset();
    while (stream.incrementToken()) {
      out.print(byteAt[offset.startOffset()]);
      out.print('\t');
      out.print(byteAt[offset.endOffset()]);
      out.print('\t');
      out.print(posInc.getPositionIncrement());
      out.print('\t');
      out.println(escape(term.toString()));
    }
    stream.end();
    stream.close();
  }

  // ---------------------------------------------------------------------------------------------
  // sentences

  private static void sentences(List<String> inputs, PrintWriter out) {
    // Exactly what HMMChineseTokenizer uses (SegmentingTokenizerBase feeds it a CharArrayIterator
    // over its buffer, which is equivalent to setText on the same chars).
    BreakIterator iterator = BreakIterator.getSentenceInstance(Locale.ROOT);
    for (int i = 0; i < inputs.size(); i++) {
      String input = inputs.get(i);
      int[] byteAt = byteOffsets(input);
      iterator.setText(input);
      out.println("# " + i);
      StringBuilder sb = new StringBuilder();
      for (int b = iterator.first(); b != BreakIterator.DONE; b = iterator.next()) {
        if (sb.length() > 0) sb.append(' ');
        sb.append(byteAt[b]);
      }
      out.println(sb);
    }
  }

  // ---------------------------------------------------------------------------------------------
  // chartypes

  private static final String[] CHAR_TYPE_NAMES = {
    "DELIMITER", "LETTER", "DIGIT", "HANZI", "SPACE_LIKE", "FULLWIDTH_LETTER", "FULLWIDTH_DIGIT",
    "OTHER", "SURROGATE"
  };

  /** One line per run: {@code <first code unit hex>\t<last code unit hex>\t<CharType name>}. */
  private static void charTypes(PrintWriter out) {
    int runStart = 0;
    int runType = Utility.getCharType((char) 0);
    for (int c = 1; c <= 0x10000; c++) {
      int type = c == 0x10000 ? -1 : Utility.getCharType((char) c);
      if (type != runType) {
        out.printf("%04X\t%04X\t%s%n", runStart, c - 1, CHAR_TYPE_NAMES[runType]);
        runStart = c;
        runType = type;
      }
    }
  }

  // ---------------------------------------------------------------------------------------------
  // dict

  private static final String HHMM = "org.apache.lucene.analysis.cn.smart.hhmm.";

  private static Object singleton(Class<?> cls) throws Exception {
    Method getInstance = cls.getDeclaredMethod("getInstance");
    getInstance.setAccessible(true);
    return getInstance.invoke(null);
  }

  private static Object field(Object instance, String name) throws Exception {
    Field f = instance.getClass().getDeclaredField(name);
    f.setAccessible(true);
    return f.get(instance);
  }

  /**
   * Header lines describe the whole dictionaries in an implementation-independent way (so the Rust
   * port can check its converted copy is complete), then one line per probe with the frequency
   * the reference implementation reports:
   *
   * <pre>
   * core.entries   &lt;count&gt;
   * core.checksum  &lt;fnv1a-64 over "word\tfreq\n" for every entry, sorted by (word as UTF-16)&gt;
   * bigram.entries &lt;count of occupied hash-table slots&gt;
   * bigram.checksum &lt;fnv1a-64 over (hash i64 LE, freq i32 LE) for every entry, sorted by hash&gt;
   * core\t&lt;word&gt;\t&lt;freq&gt;
   * bigram\t&lt;word1&gt;@&lt;word2&gt;\t&lt;freq&gt;
   * </pre>
   */
  private static void dict(List<String> probes, PrintWriter out) throws Exception {
    Class<?> wordDictClass = Class.forName(HHMM + "WordDictionary");
    Class<?> bigramDictClass = Class.forName(HHMM + "BigramDictionary");
    Object wordDict = singleton(wordDictClass);
    Object bigramDict = singleton(bigramDictClass);

    // --- core dictionary: every (word, freq), canonically ordered.
    short[] wordIndexTable = (short[]) field(wordDict, "wordIndexTable");
    char[] charIndexTable = (char[]) field(wordDict, "charIndexTable");
    char[][][] words = (char[][][]) field(wordDict, "wordItem_charArrayTable");
    int[][] freqs = (int[][]) field(wordDict, "wordItem_frequencyTable");
    List<String> entries = new ArrayList<>();
    for (int slot = 0; slot < charIndexTable.length; slot++) {
      char head = charIndexTable[slot];
      if (head == 0) continue;
      int row = wordIndexTable[slot];
      for (int j = 0; j < words[row].length; j++) {
        char[] suffix = words[row][j];
        String word = head + (suffix == null ? "" : new String(suffix));
        entries.add(word + "\t" + freqs[row][j] + "\n");
      }
    }
    entries.sort(SmartcnGolden::compareUtf16);
    long coreChecksum = FNV_OFFSET;
    for (String e : entries) coreChecksum = fnv1a(coreChecksum, e.getBytes(StandardCharsets.UTF_8));
    out.println("core.entries\t" + entries.size());
    out.printf("core.checksum\t%016x%n", coreChecksum);

    // --- bigram dictionary: every occupied slot's (hash, freq), sorted by hash.
    long[] hashes = (long[]) field(bigramDict, "bigramHashTable");
    int[] bigramFreqs = (int[]) field(bigramDict, "frequencyTable");
    List<long[]> pairs = new ArrayList<>();
    for (int i = 0; i < hashes.length; i++) {
      if (hashes[i] != 0) pairs.add(new long[] {hashes[i], bigramFreqs[i]});
    }
    pairs.sort((a, b) -> Long.compare(a[0], b[0]));
    long bigramChecksum = FNV_OFFSET;
    byte[] buf = new byte[12];
    for (long[] p : pairs) {
      for (int k = 0; k < 8; k++) buf[k] = (byte) (p[0] >>> (8 * k));
      for (int k = 0; k < 4; k++) buf[8 + k] = (byte) (p[1] >>> (8 * k));
      bigramChecksum = fnv1a(bigramChecksum, buf);
    }
    out.println("bigram.entries\t" + pairs.size());
    out.printf("bigram.checksum\t%016x%n", bigramChecksum);

    // --- probes.
    Method coreFreq = wordDictClass.getDeclaredMethod("getFrequency", char[].class);
    Method bigramFreq = bigramDictClass.getDeclaredMethod("getFrequency", char[].class);
    coreFreq.setAccessible(true);
    bigramFreq.setAccessible(true);
    for (String probe : probes) {
      if (probe.isEmpty() || probe.startsWith("//")) continue;
      int space = probe.indexOf(' ');
      String kind = probe.substring(0, space);
      String word = probe.substring(space + 1);
      Method m = kind.equals("core") ? coreFreq : bigramFreq;
      Object dict = kind.equals("core") ? wordDict : bigramDict;
      int freq = (Integer) m.invoke(dict, (Object) word.toCharArray());
      out.println(kind + "\t" + escape(word) + "\t" + freq);
    }
  }

  private static int compareUtf16(String a, String b) {
    // Plain String.compareTo compares UTF-16 code units, which is the order we want (and the order
    // the dictionary itself is sorted in), but make the intent explicit.
    return a.compareTo(b);
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
  // I/O helpers

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
    BufferedWriter w = Files.newBufferedWriter(Paths.get(path), StandardCharsets.UTF_8);
    return new PrintWriter(w) {
      @Override
      public void println() {
        write('\n'); // never platform line endings
      }
    };
  }

  /** Inverse of {@link #escape}: {@code \\}, {@code \n}, {@code \r}, {@code \t}, {@code backslash-u-brace-hex-brace}. */
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
   * (which the tokenizer produces when its 1024-unit read buffer is cut inside a surrogate pair)
   * are escaped as their code unit, e.g. {@code backslash-u-brace-d840-brace}, since they can't be
   * encoded in UTF-8; the Rust side recognises those.
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
