#include "side.h"
// WP-156 fix round 3 (L4 T2, mutation 8): a control. An explicit specialisation of a static member through a template-id qualifier (T2) whose initialiser spells a type by a typedef on side A and directly on side B. T2 writes the declaration's entries through its qualifier's last; the initialiser's type name is not written (its stream writes the type `int`): equal. (Mutation 8, T2 with the whole declaration as its range, survives this control: the initialiser's type name is not an entry of the range's holder.)
template <int> struct Q { static int s; };
namespace n { using I = int; }
#ifdef B
template <> int Q<4>::s = sizeof(int);
#else
template <> int Q<4>::s = sizeof(n::I);
#endif
