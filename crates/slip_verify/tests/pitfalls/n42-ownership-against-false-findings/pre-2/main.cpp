#include "dep.h"

namespace legacy {
struct Mine {};
}

void user() { legacy::Mine m; acme::Theirs t; }
