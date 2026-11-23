#include "side.h"
// digest-kinds §11.3: the value of the temporary r extends differs; of r,
// only its resolved unit `value` differs.
#ifdef B
constexpr int k = 2;
#else
constexpr int k = 1;
#endif
constexpr const int &r = k + 1;
