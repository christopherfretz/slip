#define CALL_INIT(PREFIX) PREFIX##_init()

namespace app {
void audio_init() {}

void start() {
  CALL_INIT(audio);
}
}
