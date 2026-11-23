#include "dep.h"
namespace acme { LEGACY_DECLARE(Mine) }
int use() { return sizeof(acme::Mine); }
