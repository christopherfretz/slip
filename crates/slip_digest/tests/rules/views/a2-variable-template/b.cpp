#include "new_dep.h"
namespace acme { template <class T> constexpr int v = sizeof(T); }
int f() { return acme::v<int>; }
