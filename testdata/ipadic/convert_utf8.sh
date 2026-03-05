#!/bin/bash

# txtファイルのみを対象とする場合
find . -name "*" -type f -exec sh -c '
  charset=$(file -i "$1" | sed "s/.*charset=//")
  case "$charset" in
    utf-8|us-ascii) 
      echo "スキップ: $1 ($charset)" ;;
    *)
      echo "処理中: $1 ($charset)"
      cp "$1" "$1.bak"
      for encoding in shift_jis euc-jp iso-2022-jp; do
        if iconv -f "$encoding" -t utf-8 "$1.bak" > "$1.tmp" 2>/dev/null; then
          mv "$1.tmp" "$1"
          echo "変換完了: $1 ($encoding → UTF-8)"
          break
        fi
      done
      rm -f "$1.tmp"
      ;;
  esac
' _ {} \;
