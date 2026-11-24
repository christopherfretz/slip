#include "dep.h"
namespace acme { static_assert(sizeof(Dep) == 1); }
int u() { return sizeof(acme::Dep); }
