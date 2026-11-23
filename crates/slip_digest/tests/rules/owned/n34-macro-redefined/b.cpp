#include "base.h"
#undef LEGACY_BEGIN
#define LEGACY_BEGIN namespace acme {
#include "dep.h"

LEGACY_BEGIN
struct Mine {};
LEGACY_END

void user() { acme::Mine m; acme::Dep d; }
