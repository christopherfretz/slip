#include "old_dep.h"
#include "new_dep.h"
namespace acme { int g(int); }
namespace other { using namespace legacy; }
using namespace acme;
int u() { return g(1); }
