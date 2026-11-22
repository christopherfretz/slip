#include "side.h"
// §10.1 LoopHintOption: `VectorizeWidth`: the fact differs.
#ifdef B
void f() {
#pragma clang loop vectorize_width(4)
  for (;;) { }
}
#else
void f() {
#pragma clang loop interleave_count(4)
  for (;;) { }
}
#endif
