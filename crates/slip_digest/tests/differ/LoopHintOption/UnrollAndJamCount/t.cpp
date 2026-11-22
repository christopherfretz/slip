#include "side.h"
// §10.1 LoopHintOption: `UnrollAndJamCount`: the fact differs.
#ifdef B
void f() {
#pragma unroll_and_jam 4
  for (;;) { }
}
#else
void f() {
#pragma unroll 4
  for (;;) { }
}
#endif
