int limit() { return 10; }

namespace inner {
int spare() { return 5; }
int f(int x) { return x + limit(); }
}
