#define CALL_INIT(PREFIX) PREFIX##_init()

namespace app {
void sound_init() {}

void start() {
  CALL_INIT(sound);
}
}
