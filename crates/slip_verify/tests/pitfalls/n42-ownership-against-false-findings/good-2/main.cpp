#include "dep.h"

namespace acme {
struct Mine {};
}

void user() { acme::Mine m; acme::Theirs t; }
