#include "old_dep.h"
#include "new_dep.h"
namespace legacy { int g(int); }
namespace L = legacy;
int u() { return L::g(1) + L::h(L::Old{}); }
