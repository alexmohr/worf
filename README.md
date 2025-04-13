# Worf

Worf is yet another dmenu style launcher, heavily inspired by wofi but written in Rust on top of GTK4. 
It supports a lot of things the same way wofi does, so migrating to worf is easy, but things I did not 
deemed necessary where dropped from worf. See breaking changes section for details.

## Setup

### Hyprland

Allow blur for Worf
```
layerrule = blur, worf
```

## Additional functionality compared to Wofi (planed)
* Support passing 'hidden' parameters that are not visible in the launcher but will be returned to the application
* Window switcher for hyprland

## Breaking changes to Wofi
* Runtime behaviour is not guaranteed to be the same and won't ever be, this includes error messages and themes.
* Themes in general are mostly compatible. Worf is using the same entity ids, 
  because worf is build on GTK4 instead of GTK3 there will be differences in the look and feel.
* Configuration files are not 100% compatible, Worf is using toml files instead, for most part this only means strings have to be quoted
* Color files are not supported

## Dropped configuration options
* stylesheet -> use style instead
* color / colors -> GTK4 does not support color files

## New options
* --fuzzy-length: Defines how long a string must be be 


## Not supported
* Wofi has a C-API, that is not and won't be supported.
