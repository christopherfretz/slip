struct P { constexpr P(int v) : v(v) {} int v; };
template <class T> struct Tr { static P r; };
template <class T> P Tr<T>::r{1};
namespace lib {
struct X {};
template <class T> auto probe() { return (void)&Tr<T>::r, 1; }
template <class T, int = sizeof(probe<T>())> void f(T, char);
}
void f(lib::X, long) {}
void use() { ::f(lib::X(), 0); }
int main() { use(); }
