struct K {};
template <class T> struct Tr { static K r; };
template <class T> K Tr<T>::r;
namespace lib {
struct X {};
template <class T> auto probe() { return (void)&Tr<T>::r, 1; }
template <class T, int = sizeof(probe<T>())> void f(T, char);
}
void f(lib::X, long) {}
void use() { f(lib::X(), 0); }
int main() { use(); }
