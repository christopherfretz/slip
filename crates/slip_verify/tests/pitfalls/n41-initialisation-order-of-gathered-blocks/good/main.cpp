#include "dep.h"

namespace acme {
int a = legacy::next();
}
int b = legacy::next();
namespace acme {
int c = legacy::next();
}

int sum() { return acme::a * 100 + b * 10 + acme::c; }
