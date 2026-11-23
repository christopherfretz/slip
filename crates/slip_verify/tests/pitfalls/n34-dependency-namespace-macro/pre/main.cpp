#include "base.h"
#include "dep.h"

LEGACY_BEGIN
struct Mine {};
LEGACY_END

void user() { legacy::Mine m; legacy::Dep d; }
