#include "old_dep.h"
namespace acme { extern "C" int oc(int); }
using namespace legacy;
int u() { return h(Old{}); }
