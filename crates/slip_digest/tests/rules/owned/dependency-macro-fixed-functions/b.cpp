#include "dep.h"
namespace acme { LEGACY_API_DECLS }
int user() { acme::legacy_init(); return acme::legacy_count(); }
