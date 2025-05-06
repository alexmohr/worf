#!/bin/bash

# A list of options, one per line
options=""
for i in $(seq 1 2000); do
  options+="Option $i"$'\n'
done

# Pipe options to wofi and capture the selection
selection=$(echo "$options" | cargo run --bin worf -- --show dmenu --sort-order default)
#selection=$(echo "$options" | wofi --show dmenu)

# Do something with the selection
echo "You selected: $selection"
