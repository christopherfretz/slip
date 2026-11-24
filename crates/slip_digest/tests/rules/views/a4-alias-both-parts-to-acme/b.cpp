#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
namespace L = acme;
int u() { return L::g(1) + legacy::h(legacy::Old{}); }
