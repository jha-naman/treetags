#!/usr/bin/env bash
function foo() { :; }
bar () { :; }
alias ll='ls -l'
alias gs="git status" xx=echo
cat <<EOF_H
hello
EOF_H
cat <<-'TAB'
	hello
TAB
source lib/helpers.sh
. ./other.sh
bash another.sh
