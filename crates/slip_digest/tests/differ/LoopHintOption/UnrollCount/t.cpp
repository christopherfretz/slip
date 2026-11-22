#include "side.h"
// §10.1 LoopHintOption: `UnrollCount`: the fact differs.
#ifdef B
void f() {
#pragma clang loop unroll_count(4)
  for (;;) { }
}
#else
void f() {
#pragma clang loop interleave_count(4)
  for (;;) { }
}
#endif
