#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
legacy::io::out &operator<<(legacy::io::out &o, const acme::Mine &) { return o; }
}

void caller() {
  legacy::io::out o;
  legacy::log(o, acme::Mine());
}
