#!/bin/sh
generated=''
for argument do
  case "$argument" in
    -oc++:*) generated=${argument#-oc++:} ;;
    *.capnp) name=${argument##*/}; printf 'controlled generated %s\n' "${name%.capnp}" > "$generated/$name.c++" ;;
  esac
done
