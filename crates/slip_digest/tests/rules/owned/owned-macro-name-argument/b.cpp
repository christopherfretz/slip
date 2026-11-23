#include "dep.h"
namespace acme { DEP_DECL(mine) }
void user() { acme::mine(); }
