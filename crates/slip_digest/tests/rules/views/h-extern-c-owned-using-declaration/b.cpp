#include "old_dep.h"
#include "new_dep.h"
namespace acme { extern "C" int cf(int); int g(int); }
using acme::cf;
int u() { return cf(1); }
