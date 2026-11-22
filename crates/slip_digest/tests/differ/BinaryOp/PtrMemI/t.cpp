#include "side.h"
// §7.2 BinaryOp `PtrMemI`.
struct S { int m; float fm; };
S s;
int S::*pm;
#ifdef B
int v = s.*pm;
#else
int v = (&s)->*pm;
#endif
