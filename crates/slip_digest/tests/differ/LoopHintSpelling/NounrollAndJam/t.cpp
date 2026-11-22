#include "side.h"
// §10.1 LoopHintSpelling: `NounrollAndJam`: the fact differs.
#ifdef B
void f() {
#pragma nounroll_and_jam
  for (;;) { }
}
#else
void f() {
#pragma nounroll
  for (;;) { }
}
#endif
