extern "C" int puts(const char *);
template <class T> struct A { int x = [] { struct L { __attribute__((constructor)) static void c() { puts("ran"); } }; return 1; }(); };
struct Base { static int s(); };
namespace n {
struct S : Base {
  template <class U = int> static auto h() { A<U> a; (void)a; return 1; }
};
}
template <class T, int = sizeof(T::s())> int call() { return 0; }
int main() { return call<n::S>(); }
