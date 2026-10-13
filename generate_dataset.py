import json
import random
from pathlib import Path

OUTPUT_FILE = Path("distillation_dataset_twopass.jsonl")

LIGHT = [
    # Korte positieve reacties
    "Lekker bezig!", "Super blij mee.", "Echt top dit.", "Heel fijn!", "Wat een opluchting.",
    "Goed geregeld.", "Trots op het resultaat.", "Mooie dag vandaag.", "Gezelligheid!",
    # Normale positieve zinnen
    "Wat een overwinning, mijn script draait eindelijk zonder bugs en alles werkt soepel!",
    "Ik ben blij dat er vooruitgang in zit.",
    "Heerlijk geslapen vannacht en weer vol energie opgestaan.",
    "De presentatie ging echt boven verwachting goed, leuke reacties gehad.",
    "Zojuist een fantastische wandeling gemaakt met de hond in het bos.",
    "Koffie in de ochtendzon, meer heb ik niet nodig om vrolijk te worden.",
    "Eindelijk die promotie binnengehaald waar ik zo hard voor heb gewerkt!",
    "Gezellig gegeten met vrienden, lang niet zo hard gelachen.",
    "Het zonnetje schijnt en ik heb er vandaag echt zin in.",
    "De app draait nu super stabiel en de feedback van gebruikers is top.",
    "Leuke spontane ontmoeting gehad met een oud-klasgenoot.",
    "Fijn weekend gehad, lekker uitgerust en opgeladen.",
    "Het eten in het restaurant was echt geweldig vanavond.",
    "Kreeg zomaar een bloemetje van mijn partner, zo lief!",
    "Alle taken voor vandaag al voor het middaguur afgevinkt, heerlijk gevoel.",
    "Mijn favoriete artiest heeft net een nieuw nummer uitgebracht, klinkt fantastisch.",
    "De sfeer op de werkvloer was vandaag ontzettend gezellig en ontspannen.",
    "Lekker hardgelopen vanochtend, nieuw persoonlijk record neergezet.",
    "Wat een fijn en open gesprek gehad met mijn leidinggevende.",
    "De taart is echt perfect gelukt, iedereen vond hem lekker.",
]

NEUTRAL = [
    # Korte feitelijke uitspraken
    "Even naar de winkel.", "Onderweg naar huis.", "Het regent buiten.", "Afspraak om 14:00.",
    "De was moet nog gedaan.", "Documenten opgeslagen.", "Geen bijzonderheden.",
    # Alledaagse praktische uitingen
    "Ik moet straks nog even langs de bouwmarkt om wat schroeven en pluggen te halen.",
    "De trein had tien minuten vertraging door werkzaamheden op het spoor.",
    "Vanmiddag even de auto voltanken en bandenspanning checken.",
    "Ik zoek nog een lekker en simpel recept voor pasta vanavond.",
    "De printer gaf weer een melding dat de gele toner bijna op is.",
    "Even snel een boterham gegeten tussen twee meetings door.",
    "Morgenmiddag staat er een routinecontrole gepland bij de tandarts.",
    "Pakketje is zojuist bezorgd en bij de buren achtergelaten.",
    "Moet nog even uitzoeken hoe die instelling in de router precies werkt.",
    "Het weerbericht voorspelt bewolking met af en toe wat motregen.",
    "Boodschappenlijstje gemaakt voor de rest van de week.",
    "De vergadering is verplaatst van woensdagochtend naar donderdagmiddag.",
    "Even gekeken naar de openingstijden van de milieustraat.",
    "Mijn fietssleutel lag nog gewoon op het aanrecht gelukkig.",
    "De batterij van mijn draadloze muis is bijna leeg, even aan de lader leggen.",
    "Ik ga vanavond een aflevering van die nieuwe documentaireserie kijken.",
    "Vandaag gewoon gewerkt en wat mails weggewerkt, niets bijzonders.",
    "De vuilnisbakken moeten vanavond nog aan de straat gezet worden.",
    "Treinkaartje gekocht via de app voor morgenochtend.",
    "Even het filter van de koffiemachine schoongemaakt.",
]

HEAVY = [
    # Korte zware uitlatingen
    "Ik kan niet meer.", "Helemaal op.", "Zo ontzettend moe.", "Alles voelt zwaar.",
    "Zie het niet meer zitten.", "Diep verdrietig.", "Constant angst.", "Volledig leeg.",
    # Complexe zware uitingen
    "Ik zie door de bomen het bos niet meer, het stapelt zich alleen maar op en ik breek bijna.",
    "Alles zit tegen vandaag, ik voel me compleet uitgeput en moedeloos.",
    "Ik pieker me suf 's nachts en kom maar niet aan rust toe.",
    "Het gevoel dat ik overal tekortschiet en iedereen teleurstel vreet aan me.",
    "Weer een paniekaanval gehad op werk, ik weet even niet meer hoe ik dit volhoud.",
    "De eenzaamheid drukt zwaar vandaag, de muren komen echt op me af.",
    "Elke dag voelt als overleven en ik mis het perspectief om door te gaan.",
    "Zoveel verdriet en spanning in mijn lijf, ik kan niet stoppen met huilen.",
    "Doodmoe van het constant moeten doen alsof alles prima met me gaat.",
    "Ik voel me volkomen leeg van binnen en nergens meer gemotiveerd voor.",
    "De werkdruk wordt me echt te veel, ik slaap slecht en functioneer nauwelijks.",
    "Ik raak mezelf helemaal kwijt in deze situatie en zie geen uitweg meer.",
    "Het voelt alsof niemand me begrijpt en ik er helemaal alleen voor sta.",
    "De angst verlamt me volledig vandaag, kan amper fatsoenlijk ademhalen.",
    "Waarom lukt er nou nooit eens iets, ik voel me een complete mislukking.",
    "Ik wil gewoon rust in mijn hoofd, de maalstroom van gedachten stopt maar niet.",
    "Het verdriet na het verlies is vandaag weer zo intens rauw aanwezig.",
    "Mentaal zit ik echt aan mijn absolute limiet, er hoeft maar dít te gebeuren.",
    "Ik sleep me door de dagen heen en heb nergens meer plezier in.",
    "Zoveel fysieke en mentale spanning opgebouwd dat mijn hele lijf pijn doet.",
]

def generate():
    data = []
    # Voeg basiszinnen toe
    for t in LIGHT:
        data.append({"text": t, "ground_truth": {"category": "light"}})
    for t in NEUTRAL:
        data.append({"text": t, "ground_truth": {"category": "neutral"}})
    for t in HEAVY:
        data.append({"text": t, "ground_truth": {"category": "heavy"}})

    # Vermenigvuldig met lichte variatie/herhaling om een trainingsset van ~900 items te krijgen
    expanded = []
    for _ in range(15):
        for item in data:
            expanded.append(item)

    random.seed(42)
    random.shuffle(expanded)

    with open(OUTPUT_FILE, "w", encoding="utf-8") as f:
        for item in expanded:
            f.write(json.dumps(item, ensure_ascii=False) + "\n")

    print(f"Gereed: {len(expanded)} samples in {OUTPUT_FILE}")

if __name__ == "__main__":
    generate()