#include "dep.h"
namespace legacy { LEGACY_API_DECLS }
int user() { legacy::legacy_init(); return legacy::legacy_count(); }
