#include "side.h"
// §10.1 LoopHintSpelling: `UnrollAndJam`: the fact differs.
#ifdef B
void f() {
#pragma unroll_and_jam
  for (;;) { }
}
#else
void f() {
#pragma unroll
  for (;;) { }
}
#endif
