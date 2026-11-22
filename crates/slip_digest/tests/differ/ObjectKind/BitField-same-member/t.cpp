#include "side.h"
// §7.2 ObjectKind `BitField`: the same member, a bit-field on one side
// only; `v` differs by its object kind alone (keys are not content).
struct S {
#ifdef B
  int m;
#else
  int m : 3;
#endif
};
S s;
int v = s.m;
