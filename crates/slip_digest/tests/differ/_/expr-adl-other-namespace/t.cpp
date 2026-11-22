#include "side.h"
// digest §6 _ `expr-adl-other-namespace`.
namespace n1 { struct Kb {}; int f(Kb); }
namespace n2 { struct K : n1::Kb {};
#ifdef B
int f(K); }
n2::K k;
int v = f(k);
#else
}
n2::K k;
int v = f(k);
#endif
