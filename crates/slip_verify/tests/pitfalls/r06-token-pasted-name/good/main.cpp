#define CALL_INIT(PREFIX) PREFIX##_init()

void sound_init() {}

void start() {
  CALL_INIT(sound);
}
