#include "side.h"
// digest §6 _ `expr-adl-other-operator`: ADL alone selects
// `n1::operator+(const K &, long)` (A); ordinary lookup through a using-directive
// adds `n2::operator+(const K &, int)`, a better match (B). The call differs by its
// callee, which the ADL bit alone would not show.
namespace n1 { struct K {}; int operator+(const K &, long); }
namespace n2 { int operator+(const n1::K &, int); }
n1::K k;
#ifdef B
using namespace n2;
#endif
int v = k + 0;
