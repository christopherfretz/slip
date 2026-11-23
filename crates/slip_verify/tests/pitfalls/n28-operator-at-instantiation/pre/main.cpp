#include "dep.h"

namespace legacy {
struct Mine {};

legacy::io::out &operator<<(legacy::io::out &o, const Mine &) { return o; }
}

void caller() {
  legacy::io::out o;
  legacy::log(o, legacy::Mine());
}
