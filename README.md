# Worf - Wayland Optimized Run Facilitator

Worf is yet another style launcher, heavily inspired by wofi, rofi and walker.
Worf is written in Rust on top of GTK4. 

It aims to be a drop in replacement for wofi in most part, so it is (almost) compatible with its 
configuration and css files. See below for differences



## Setup

### Hyprland

Allow blur for Worf
```
layerrule = blur, worf
```

## Additional functionality compared to Wofi (planed)
* Support passing 'hidden' parameters that are not visible in the launcher but will be returned to the application
* Window switcher for hyprland
* All arguments expect show are supported by config and args

### New config / command line options
* fuzzy-length: Defines how long a string must be to be considered for fuzzy match
* row-box-orientation: Allows aligning values vertically to place the label below the icon
* text wrapping
* configurable animations

### New Styling options
* `label`: Allows styling the label
* `row`: Allows styling to row, mainly used to disable hover effects

## Breaking changes to Wofi
* Runtime behaviour is not guaranteed to be the same and won't ever be, this includes error messages and themes.
* Themes in general are mostly compatible. Worf is using the same entity ids, 
  because worf is build on GTK4 instead of GTK3 there will be differences in the look and feel.
* Configuration files are not 100% compatible, Worf is using toml files instead, for most part this only means strings have to be quoted
* Color files are not supported
* `line_wrap` is now called `line-wrap`

## Dropped arguments
* `mode`, use show
* `dmenu`, use show
* `D`, arguments are the same as config in worf, no need to have this flag.

### Dropped configuration options
* stylesheet -> use style instead
* color / colors -> GTK4 does not support color files



## Not supported
* Wofi has a C-API, that is not and won't be supported.
