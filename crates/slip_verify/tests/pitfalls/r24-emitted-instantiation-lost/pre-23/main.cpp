extern "C" int puts(const char *);
template <class T> struct A { static constexpr int k = [] { struct L { __attribute__((constructor)) static void c() { puts("ran"); } }; return 1; }(); };
struct Base { static int s(); };
namespace n {
struct S : Base {
  template <class U = int> static auto h() { (void)&A<U>::k; return 1; }
};
}
template <class T, int = sizeof(T::s())> int call() { return 0; }
int main() { return call<n::S>(); }
