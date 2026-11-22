#include "side.h"
// §10.1 LoopHintOption: `VectorizePredicate`: the fact differs.
#ifdef B
void f() {
#pragma clang loop vectorize_predicate(enable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize(enable)
  for (;;) { }
}
#endif
