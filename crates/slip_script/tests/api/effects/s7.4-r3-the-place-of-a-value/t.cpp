#define ID(x) x
#define CALL_G g(1) + 0
template <class T> T tf(T);
int g(int);
int a = g(2);
int b = ID(g)(3);
int c = CALL_G;
int d = tf(4);
