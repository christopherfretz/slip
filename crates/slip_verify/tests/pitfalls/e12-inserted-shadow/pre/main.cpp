int limit() { return 10; }

namespace inner {

int f(int x) { return x + limit(); }
}
