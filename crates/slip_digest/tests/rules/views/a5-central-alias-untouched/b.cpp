#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
namespace L = legacy;
int u() { return acme::g(1) + L::h(L::Old{}); }
