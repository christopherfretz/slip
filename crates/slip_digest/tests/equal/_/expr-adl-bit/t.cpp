#include "side.h"
// digest §6 _ `expr-adl-bit`: the same `operator+` found by ADL alone (A:
// the using-directive comes after the use; Kiln sets the call's ADL bit,
// overload/oper.rs:1160) and by ordinary lookup too (B: the directive
// comes before; the bit is clear). The bit is `out`.
namespace n1 { struct K {}; int operator+(const K &, long); }
n1::K k;
#ifdef B
using namespace n1;
int v = k + 0;
#else
int v = k + 0;
using namespace n1;
#endif
