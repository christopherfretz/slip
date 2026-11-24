#include "new_dep.h"
namespace legacy { template <class T> constexpr int v = sizeof(T); }
int f() { return legacy::v<int>; }
