extern "C" int puts(const char *);
template <class T> constexpr int vt = [] { struct L { __attribute__((used)) static void c() { puts("ran"); } }; return 1; }();
struct Base { static int s(); };
namespace n {
struct S : Base {
  template <class U = int> static auto s() { (void)&vt<U>; return 1; }
};
}
template <class T, int = sizeof(T::s())> int call() { return 0; }
int main() { return call<n::S>(); }
