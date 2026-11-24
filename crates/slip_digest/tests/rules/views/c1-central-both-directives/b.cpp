#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
using namespace acme;
using namespace legacy;
int u() { return g(1) + h(Old{}); }
