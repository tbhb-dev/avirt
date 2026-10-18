"""Word lists for jargon_check.py: stopwords, ordinary English words and ordinary compounds."""

STOPWORDS = frozenset(
    """
a about above after again against all almost along also although always am among an and another any anyone
anything are aren't around as at be because been before being below between both but by can can't cannot could
couldn't did didn't do does doesn't doing don't down during each either else enough etc even ever every few for
from further had hadn't has hasn't have haven't having he her here hers him his how however i if in into is isn't
it its itself just least less like may me might more most much must mustn't my neither never no nor not of off
often on once one only onto or other others otherwise our ours out over own per rather same shall shan't she
should shouldn't since so some something still such than that the their theirs them then there therefore these
they this those though through thus to too toward towards under unless until up upon us very via was wasn't we
were weren't what whatever when whenever where whereas whether which while who whom whose why will with within
without won't would wouldn't yet you your yours
""".split()
)

COMMON_WORDS = frozenset(
    """
ability able accept access according account across act action active actual add address after again against age
ago agree air allow almost alone along already also although always among amount analysis and animal another answer
any appear apply approach area argue arm around arrive art article ask assume attack attention attempt author
available avoid away baby back bad bag ball bank bar base basic be bear beat beautiful because become bed before
begin behavior behind believe benefit best better between beyond big bill bit black blood blue board body book born
both box boy break bring brother budget build building business buy call camera campaign can capital car card care
career case catch cause cell center central century certain certainly chair challenge chance change character
charge check child choice choose church citizen city civil claim class clear clearly close coach cold collection
college color come commercial common community company compare computer concern condition conference consider
consumer contain continue control cost could country couple course court cover create crime cultural culture cup
current customer cut dark data daughter day dead deal death debate decade decide decision deep defense degree
democrat democratic describe design despite detail determine develop development die difference different
difficult dinner direction director discover discuss discussion disease do doctor dog door down draw dream drive
drop drug during each early east easy eat economic economy edge education effect effort eight either election else
employee end energy enjoy enough enter entire environment environmental especially establish even evening event
ever every everybody everyone everything evidence exactly example executive exist expect experience expert explain
eye face fact factor fail fall family far fast father fear federal feel feeling few field fight figure fill film
final finally financial find fine finger finish fire firm first fish five floor fly focus follow food foot for force
foreign forget form former forward four free friend from front full fund future game garden gas general generation
get girl give glass go goal good government great green ground group grow growth guess gun guy hair half hand hang
happen happy hard have he head health hear heart heat heavy help her here herself high him himself his history hit
hold home hope hospital hot hotel hour house how however huge human hundred husband idea identify if image imagine
impact important improve in include including increase indeed indicate individual industry information inside
instead institution interest interesting international interview into investment involve issue it item its itself
job join just keep key kid kill kind kitchen know knowledge land language large last late later laugh law lawyer lay
lead leader learn least leave left leg legal less let letter level lie life light like likely line list listen
little live local long look lose loss lot love low machine magazine main maintain major majority make man manage
management manager many market marriage material matter may maybe me mean measure media medical meet meeting member
memory mention message method middle might military million mind minute miss mission model modern moment money month
more morning most mother mouth move movement movie mr mrs much music must my myself name nation national natural
nature near nearly necessary need network never new news newspaper next nice night no none nor north not note
nothing notice now number occur of off offer office officer official often oh oil ok old on once one only onto open
operation opportunity option or order organization other others our out outside over own owner page pain painting
paper parent part participant particular particularly partner party pass past patient pattern pay peace people per
perform performance perhaps period person personal phone physical pick picture piece place plan plant play player
point police policy political politics poor popular population position positive possible power practice prepare
present president pressure pretty prevent price private probably problem process produce product production
professional professor program project property protect prove provide public pull purpose push put quality question
quickly quite race radio raise range rate rather reach read ready real reality realize really reason receive recent
recently recognize record red reduce reflect region relate relationship religious remain remember remove report
represent republican require research resource respond response responsibility rest result return reveal rich right
rise risk road rock role room rule run safe same save say scene school science scientist score sea season seat
second section security see seek seem sell send senior sense series serious serve service set seven several shake
share she shoot short shot should shoulder show side sign significant similar simple simply since sing single
sister sit site situation six size skill skin small smile so social society soldier some somebody someone something
sometimes son song soon sort sound source south southern space speak special specific speech spend sport spring
staff stage stand standard star start state statement station stay step still stock stop store story strategy
street strong structure student study stuff style subject success successful such suddenly suffer suggest summer
support sure surface system table take talk task tax teach teacher team technology television tell ten tend term
test than thank that the their them themselves then theory there these they thing think third this those though
thought thousand threat three through throughout throw thus time to today together tonight too top total tough
toward town trade traditional training travel treat treatment tree trial trip trouble true truth try turn tv two
type under understand unit until up upon us use usually value various very victim view violence visit voice vote
wait walk wall want war watch water way we weapon wear week weight well west western what whatever when where
whether which while white who whole whom whose why wide wife will win wind window wish with within without woman
wonder word work application device destination signature port worker world worry would write writer wrong yard yeah year yes yet you young your yourself
allowed always another apart appears asked automatically becomes begins behaves better block body bound bytes
  changes checks closes closed comes contains copy copies count counts creates currently default
defined defines delete depends describes details done during either ends ensure error errors exactly exists fails
failed failure fetch fetches file files first fixed given goes handle handles happens holds ignore includes
instead keeps known later launch launches lets limit limits loaded lock logs makes means missing needs normal
notes opens output outputs owns passes path paths plain plus point points reads reached reaches refuses relies
removed removes replace replaces request requests requires returned returns reuse runs sends sent sets shared
shows simple starts stops stored stores takes tells there's thing things tries unchanged updates used uses using
value values waits wants whole works writes written
""".split()
)

# Ordinary hyphenated English that no project would coin.
COMMON_COMPOUNDS = frozenset(
    """
built-in by-product case-by-case end-to-end first-class follow-up front-end back-end high-level low-level
long-lived short-lived long-running long-term short-term real-time read-only read-write run-time one-time
one-to-one one-to-many many-to-many on-demand on-disk in-memory in-process out-of-process off-by-one
opt-in opt-out per-request per-process so-called up-to-date well-known well-defined well-formed
well-behaved up-front step-by-step trade-off tradeoff stand-alone set-up life-cycle look-up
re-use re-run re-read re-open re-check re-create re-send re-sign re-issue non-empty non-zero
x-forwarded-for client-side server-side host-side guest-side user-space kernel-space
fail-closed fail-open open-source third-party single-use multi-line single-line read-after-write
all-in-one stand-by hand-off round-trip time-out time-of-check time-of-use cross-check command-line ad-hoc
""".split()
)

COMPOUND_PREFIXES = frozenset(
    "anti auto co counter de dis ex extra inter intra mid mis multi non out over post pre re semi sub super un under".split()
)

# Stems of common verbs. A repeated phrase holding one of these (in any inflection) is a sentence
# fragment ("omits this capability"), not a coined noun phrase.
VERB_STEMS = frozenset(
    """
accept add affect agree allow answer appear apply approve ask assume avoid become begin believe bring build call
cause change check choose close come consider contain continue cover create decide define depend describe
determine develop differ do drop eat enable end ensure enter exclude exist expect explain fail fall feel fetch find
follow forbid forget get give go grant handle happen have hear help hold hope identify ignore include increase
indicate involve keep know lack launch lead learn leave let lie like limit listen live look lose make mean meet
mention miss move need note notice occur offer omit open pass pay permit place play prefer prevent produce provide
pull put reach read receive record reduce refuse reject relate remain remember remove replace report represent
require respond return reveal run say see seek seem send serve set share show sit spawn specify spend stand start
stay stop store suggest take talk tell tend think treat try turn understand update use verify wait want watch work
write
authenticate authorize bind bypass compose conform deliver deny dial emit enforce expire expose forward freeze
log manage mount operate persist publish refer renew restart resume retain revoke rotate route sign spawn
supervise transmit validate
""".split()
)
