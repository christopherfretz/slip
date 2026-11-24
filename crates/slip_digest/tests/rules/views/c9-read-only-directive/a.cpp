#include "old_dep.h"
#include "new_dep.h"
namespace legacy { int g(int); }
#include "ro_use.h"
int u() { return g(1) + h(Old{}); }
