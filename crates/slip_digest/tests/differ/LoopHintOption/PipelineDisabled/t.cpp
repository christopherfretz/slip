#include "side.h"
// §10.1 LoopHintOption: `PipelineDisabled`: the fact differs.
#ifdef B
void f() {
#pragma clang loop pipeline(disable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize(disable)
  for (;;) { }
}
#endif
