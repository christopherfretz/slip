// WP-104 (digest §9), review M2h: a namespace-scope constant holding the name of a local class's constructor; the name check keeps L and K apart
consteval const char *f() {
  struct L { const char *p; constexpr L() : p(__func__) {} };
  return L().p;
}
constexpr const char *G = f();
