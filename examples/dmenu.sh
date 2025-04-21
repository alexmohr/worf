#!/bin/bash

# A list of options, one per line
options="Option 1
Option 2
Option 3"

# Pipe options to wofi and capture the selection
selection=$(echo "$options" | cargo run -- --show dmenu)

# Do something with the selection
echo "You selected: $selection"
