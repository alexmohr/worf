# Worf Warden

Simple password manager build upon these additional tools aside worf
* [rbw](https://github.com/doy/rbw) 
* 
  * [pinentry](https://www.gnupg.org/related_software/pinentry/index.en.html) is required to show a dialog show password entry 
  * As worf warden 
* [ydotool](https://github.com/ReimuNotMoe/ydotool)
  * ydotool is just the defaults, other tools can be configured via `--typing-cmd` or using this key in the config file. 

The idea it taken from https://github.com/mattydebie/bitwarden-rofi/blob/master/bwmenu

## Custom auto typing

Custom key strokes are supported for auto typing. 
For example this can be used for some websites like PayPal, 
where `<enter>` after the username must be typed instead of `<tab>`

Special variables:
* `$U` -- Username
* `$P` -- Password
* `$T` -- Two factor
* `$S` -- Sleep in milliseconds
* `_` -- All underscores are removed and used to make the string more readable

The default is `$U\t$P` which is user, tab, password.  
As the string is passed to the typing tool, see their documentation for special chars.

## Configuration

The location of the configuration file follows the same rules as worf itself.

```toml
typing_cmd = "ydotool"
typing_cmd_args = ["type"]

[custom_auto_types]
# This will use User, enter, password for the demo entry.
# You can use the id or the label as key, where id has higher precedence.
Demo = "$U\n$P"
# Will sleep 500ms before typing password
# Any underscore will be ignored
Delayed = "$U_\n_$S_500_$P"
```




![example](../images/worf-warden.png)
