#!/bin/bash
NAME=$(basename "$0")
case "$1" in
  list)
      for i in {1..1500}; do
          printf '%s-%04g\t1.0.0\n' "$NAME" "$i"
      done
      exit 0
      ;;
  install)
      exit 0
      ;;
  remove)
      exit 0
      ;;
  prepare)
      exit 0
      ;;
  update-list)
      # means update-list is unsupported by this plugin
      exit 1
      ;;
  finalize)
      exit 0
      ;;
  *)
     exit 0
     ;;
esac
