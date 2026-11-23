#include "dep.h"

namespace legacy {
int a = legacy::next();
}
int b = legacy::next();
namespace legacy {
int c = legacy::next();
}

int sum() { return legacy::a * 100 + b * 10 + legacy::c; }
