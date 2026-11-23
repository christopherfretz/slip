// WP-104 (digest §9), review G4: the owner of a name string is found in the whole function definition (function-try-block, mem-initializers)
void take(const char *);
consteval const char *id(const char *p) { return p; }
void L() {
  struct L { const char *p; L() : p(id(__func__)) {} };
  take(L().p);
}
