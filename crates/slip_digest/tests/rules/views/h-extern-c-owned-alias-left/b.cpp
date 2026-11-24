#include "old_dep.h"
namespace acme { extern "C" int oc(int); }
namespace L = legacy;
int u() { return L::h(L::Old{}); }
