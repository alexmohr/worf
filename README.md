# Worf

Worf is a clone of [wofi](https://github.com/SimplyCEO/wofi) written in rust. 
Although no code was taken over, the original project is great and to honor their license this tool is licensed under the same GPLV3 terms.

* Wofis css files are supported
* Wofis command line flags are supported

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
* Error messages differ
* Configuration files are not 100% compatible, Worf is using toml files instead, for most part this only means strings have to be quoted


## Not supported
* Wofi has a C-API, that is not and won't be supported. As of now there are no plans to provide a Rust API either.
