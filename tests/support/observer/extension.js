import Clutter from 'gi://Clutter';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
export default class Observer extends Extension {
    enable() {
        global.context.unsafe_mode = true;
        global.shikomiTestKeyboard = Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    }
    disable() { global.shikomiTestKeyboard = null; global.context.unsafe_mode = false; }
}
