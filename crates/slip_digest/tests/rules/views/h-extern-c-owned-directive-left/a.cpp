#include "old_dep.h"
namespace legacy { extern "C" int oc(int); }
using namespace legacy;
int u() { return h(Old{}); }
