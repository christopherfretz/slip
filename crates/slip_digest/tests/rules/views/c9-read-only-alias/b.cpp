#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
#include "ro_use.h"
int u() { return acme::g(1) + L::h(L::Old{}); }
