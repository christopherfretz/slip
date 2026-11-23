namespace legacy {
inline long run(long n) { return n; }
}
#define LEGACY_CALL(f, x) legacy::f(x)
