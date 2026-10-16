"""Tally light for one camera on an ESP32 (MicroPython), over lmix's text
protocol (`lmix-core --tcp 0.0.0.0:8201`): red while the input is on
program, green while it is in preview.

Copy to the board as main.py after setting the values below. Under
CPython (no `machine`, `network`) it prints the light instead, for a try
on the desk:  python3 main.py
"""
import socket
import time

LMIX = ("192.168.1.10", 8201)   # lmix-core's --tcp address
INPUT = 1                       # input number in the bar
RED_PIN, GREEN_PIN = 25, 26     # GPIOs of the LEDs
WIFI = ("my-network", "secret")

try:
    import machine
    import network
except ImportError:  # on the desk
    machine = network = None


class Light:
    def __init__(self):
        if machine:
            self.red = machine.Pin(RED_PIN, machine.Pin.OUT)
            self.green = machine.Pin(GREEN_PIN, machine.Pin.OUT)
        self.state = None

    def show(self, state):
        """`program`, `preview` or `off`."""
        if state == self.state:
            return
        self.state = state
        if machine:
            self.red.value(state == "program")
            self.green.value(state == "preview")
        else:
            print("tally", state)


def connect_wifi():
    if not network:
        return
    wlan = network.WLAN(network.STA_IF)
    wlan.active(True)
    if not wlan.isconnected():
        wlan.connect(*WIFI)
        while not wlan.isconnected():
            time.sleep(0.2)


KEEPALIVE_S = 5                 # ask for the tally this often
TIMEOUT_S = 12                  # no answer this long: lmix is gone, go dark


def follow(light):
    sock = socket.socket()
    sock.connect(socket.getaddrinfo(LMIX[0], LMIX[1])[0][-1])
    # A connection that dies without closing (power, Wi-Fi) must not leave
    # the light on: lmix is asked for the tally every few seconds, and
    # silence for longer than TIMEOUT_S counts as gone.
    sock.settimeout(KEEPALIVE_S)
    buffer = b""
    heard = time.time()
    greeted = False
    try:
        sock.send(b"SUB TALLY\nTALLY\n")
        while True:
            try:
                data = sock.recv(256)
            except OSError as e:  # a read timeout: ask again
                if "timed out" not in str(e) and getattr(e, "errno", None) not in (110, 116):
                    raise
                if time.time() - heard > TIMEOUT_S:
                    raise OSError("lmix does not answer")
                sock.send(b"TALLY\n")
                continue
            if not data:
                raise OSError("closed")
            heard = time.time()
            buffer += data
            while b"\n" in buffer:
                line, buffer = buffer.split(b"\n", 1)
                line = line.strip()
                if not greeted:
                    if not line.startswith(b"LMIX "):
                        raise OSError("not lmix")
                    greeted = True
                elif line.startswith(b"TALLY "):
                    value = line[6:][INPUT - 1:INPUT]
                    light.show({b"1": "program", b"2": "preview"}.get(value, "off"))
    finally:
        sock.close()


def main():
    light = Light()
    while True:
        try:
            connect_wifi()
            follow(light)
        except OSError as e:
            # lmix restarted or the network is gone: dark, then again.
            light.show("off")
            print("lmix:", e)
            time.sleep(2)


main()
