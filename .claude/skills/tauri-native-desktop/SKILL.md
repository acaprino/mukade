---
name: tauri-native-desktop
description: "Audit e miglioramento dell'integrazione desktop delle app Tauri: finestre e dialoghi OS, convenzioni Windows/macOS/Linux, accessibilità e ciclo di vita. Usare per rendere Tauri più nativo, non per il solo restyling web o per app mobile."
---

# Tauri Native Desktop

Agisci da ingegnere dell'integrazione desktop. Ottimizza il comportamento osservabile sul sistema operativo scelto, mantenendo architettura e autorizzazioni del progetto. La somiglianza grafica non sostituisce focus, tastiera, accessibilità e ciclo di vita corretti.

## Individua il livello richiesto

Distinguere sempre:

- **Controllo nativo OS**: file picker, message box, menu, tray; usa API del sistema, direttamente o tramite plugin verificato.
- **Finestra nativa con contenuto web**: finestra Tauri separata gestita dal window manager; il modulo dentro resta HTML/CSS.
- **Interfaccia interamente nativa**: widget AppKit, WinUI/Win32 o GTK; richiede integrazione specifica o una diversa scelta di toolkit. Tauri non converte automaticamente React in questi controlli.

Per moduli fuori dalla principale il secondo livello è spesso appropriato. Se servono anche widget nativi, esplicita il costo e verifica l'integrazione prima di prometterla.

## Procedura

1. **Rileva il contesto.** Leggi istruzioni locali, manifesti e lockfile. Identifica piattaforme distribuite, versioni Tauri/plugin/WebView, finestre, eventuale frontend web condiviso, autenticazione, stato applicativo e ACL. Chiedi solo ciò che cambia la soluzione e non si ricava dal progetto.
2. **Classifica le superfici.** Inventaria dialoghi, pannelli, menu e avvisi. Scegli per ciascuno controllo OS, finestra Tauri o superficie interna. Registra motivazione e fallback. La richiesta di dialoghi esterni non trasforma automaticamente tooltip e menu a discesa in finestre.
3. **Definisci il contratto desktop.** Leggi [la guida](references/guide.md) e [la checklist](references/acceptance.md). Definisci proprietario, modalità, dimensionamento, focus, chiusura, risultato e stato autorevole. Verifica sulle versioni effettive: `parent`, `owner`, `transient_for`, sheet e modalità non sono sinonimi.
4. **Realizza l'ambito autorizzato.** In un audit consegna evidenze e priorità. Quando sono richieste modifiche, implementa incrementi verificabili riusando dominio e trasporto esistenti. Definisci i privilegi minimi delle nuove finestre e testa il chiamante. Mantieni il percorso browser quando il prodotto lo richiede.
5. **Verifica nell'app reale.** Esegui controlli automatici pertinenti e prove nell'app installata sulle piattaforme dichiarate. Distingui codice ispezionato, compilazione, test automatici e comportamento osservato. Segnala le prove che richiedono ancora un ambiente diverso.

## Decisioni essenziali

- Preferisci cornice OS quando la priorità è il comportamento nativo e il progetto non richiede una barra propria. Se mantieni la barra personalizzata, verifica le funzioni di sistema che sostituisce.
- Un dialogo modale richiede ripristino affidabile della principale su conferma, annullamento, X, errore e distruzione. `always_on_top` non realizza questo contratto.
- Convertire un dialogo in finestra OS trasferisce anche il suo intero ciclo di vita: apertura, disponibilità, conferma/salvataggio, annullamento, Escape, X/Alt+F4, chiusura dell'owner, errore e shutdown. Un salvataggio riuscito deve chiudere la finestra OS, non soltanto smontare il contenuto React: una finestra vuota è un difetto. Se il salvataggio fallisce, restano bozza, messaggio e possibilità di riprovare. Verificare anche rilascio della modalità, ritorno del focus e cleanup una sola volta; non assumere che `window.close()`, `close()`, `destroy()` e lo smontaggio React seguano lo stesso percorso nel runtime scelto.
- Un modulo tagliato dal frame della principale richiede una superficie esterna appropriata. Provare finestre piccole, testo ingrandito e contenuto lungo: corpo scorrevole e azioni raggiungibili, senza dipendere dai bordi della principale.
- Un'altra WebView ha un contesto JavaScript separato. Usa il servizio/host già autorevole, lettura iniziale affidabile ed eventi mirati di invalidazione. Evita un secondo backend e segreti negli URL. Se solo main accede alle credenziali, definisci il percorso autenticato della secondaria prima di implementarla; la guida confronta relay e broker.
- Applica il bootstrap desktop pertinente a ogni nuova WebView, non solo a main. Una Settings può essere non modale e riutilizzabile; il fatto che sia esterna non impone un blocco della principale.
- Conserva editing, selezione utile, IME e ingrandimento accessibile mentre rimuovi i comandi browser non pertinenti. Sostituisci esplicitamente le scorciatoie intercettate.
- Valuta esplicitamente il bounce: distingui overscroll/rubber-banding da animazioni elastiche. Scegli per superficie e piattaforma, preservando lo scorrimento e le preferenze di movimento; la guida contiene criteri e CSS mirato.
- Mica/blur, tray, avvio automatico, scorciatoie globali e updater sono scelte del prodotto, non un pacchetto obbligatorio per la natività.

## Consegna

Mostra prima il comportamento ottenuto o il difetto prioritario. Includi file e righe, piattaforma/versione, verifiche e limiti. Per un audit usa `superficie → stato attuale → soluzione → prova di accettazione`. Una sola azione concreta finale se resta lavoro affidato all'utente.

Le fonti primarie nella guida descrivono la ricerca del 2026-11-11. Ricontrolla le API necessarie per ogni nuovo progetto, soprattutto modalità, notifiche e permessi. La skill fornisce competenze: non autorizza da sola modifiche, installazioni o pubblicazioni.
