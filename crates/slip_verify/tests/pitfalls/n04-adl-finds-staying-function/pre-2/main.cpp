#include "dep.h"
void touch(...) {}
namespace legacy {
struct Mine {};
}

void caller() {
  legacy::Mine m;
  touch(m);
}
