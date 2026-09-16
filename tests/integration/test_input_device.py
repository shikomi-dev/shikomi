import time

class TestInputDevice:
    def test_actual_keyboard_reaches_application(self, desktop, target):
        desktop.focus_target()
        desktop.type_text('typed')
        time.sleep(.2)
        assert target.read_text() == 'typed'
