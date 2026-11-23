#include "side.h"
// digest-kinds §11.1 property 7: Kiln evaluates the immediate invocation
// in a default argument written in source at its first use
// (kiln_sema consteval.rs); the resolved unit is the function's, with the
// parameter's index.
consteval int id(int x) { return x; }
void f(int a = id(3));
#ifdef B
void h() { f(); }
#endif
