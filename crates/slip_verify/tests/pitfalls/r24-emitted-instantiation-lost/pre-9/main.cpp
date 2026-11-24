extern "C" int puts(const char *);
struct Base { static int s(); };
namespace n {
struct S : Base {
  template <class U = int> __attribute__((constructor)) static auto s() { puts("ran"); return 1; }
};
}
template <class T, int = sizeof(T::s())> int call() { return 0; }
int main() { return call<n::S>(); }
