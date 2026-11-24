template <class T> struct Tr { static constexpr bool value = sizeof(T) > 0; };
namespace lib {
struct X {};
template <class T, int = sizeof(char[Tr<T>::value ? 1 : 2])> void f(T, char);
}
void f(lib::X, long) {}
void use() { ::f(lib::X(), 0); }
int main() { use(); }
