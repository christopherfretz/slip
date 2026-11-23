#include "dep.h"
#define OWN_DECL(n) void n();
namespace acme { OWN_DECL(DEP_NAME) }
void user() { acme::legacy_init(); }
