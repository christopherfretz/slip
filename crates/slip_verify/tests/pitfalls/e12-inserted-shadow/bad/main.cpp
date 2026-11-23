int limit() { return 10; }

namespace inner {
int limit() { return 5; }
int f(int x) { return x + limit(); }
}
