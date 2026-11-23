#define CALL_INIT(PREFIX) PREFIX##_init()
void audio_init() {}
namespace app {
void sound_init() {}

void start() {
  CALL_INIT(sound);
}
}
