#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
using namespace legacy;
int u() { return acme::g(1) + h(Old{}); }
