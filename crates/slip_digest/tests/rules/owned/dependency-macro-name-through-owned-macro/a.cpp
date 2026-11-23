#include "dep.h"
#define OWN_DECL(n) void n();
namespace legacy { OWN_DECL(DEP_NAME) }
void user() { legacy::legacy_init(); }
